//! NATS message queue integration for Pent Agent.
//!
//! Run notifications are advisory and the database remains the source of
//! truth for run state. Conversation recipe tools additionally use an
//! explicit request/reply subject so security binaries only run in pent-agent.
//!
//! ## Queue tiers
//!
//! | Tier | Subject | Typical workloads |
//! |---|---|---|
//! | `scan.light` | `penta.scan.light` | http_probe, dns_lookup, web_crawl |
//! | `scan.heavy` | `penta.scan.heavy` | nmap, nuclei, dir_fuzz |
//! | `native.fast` | `penta.native.fast` | built-in fast tools |
//! | `browser` | `penta.browser` | browser-automation tools |
//! | `dangerous` | `penta.dangerous` | high-risk tools (manual approval) |
//! | `report` | `penta.report` | report generation |

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use async_nats::Client;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};

use crate::infrastructure::process::{NetworkPolicy, ToolExecutionLimits};

const WORKER_PROBE_TIMEOUT_SECS: u64 = 2;
const EXECUTION_HEARTBEAT_INTERVAL_SECS: u64 = 2;
const EXECUTION_HEARTBEAT_GRACE_SECS: u64 = 7;
const MAX_REMOTE_OUTPUT_BYTES: usize = 400 * 1024;

struct RemoteCancellationGuard(Option<(Client, String)>);

struct AbortTask(tokio::task::JoinHandle<()>);

impl Drop for AbortTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl RemoteCancellationGuard {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for RemoteCancellationGuard {
    fn drop(&mut self) {
        if let Some((client, subject)) = self.0.take() {
            tokio::spawn(async move {
                let _ = client.publish(subject, Vec::new().into()).await;
            });
        }
    }
}

/// NATS-backed event bus for run notifications.
///
/// Falls back to a no-op mode when `NATS_URL` is unset or the connection
/// fails — workers will poll the DB directly.
pub struct NatsEventBus {
    client: Option<Client>,
}

impl NatsEventBus {
    /// Connect to NATS. Returns a no-op bus when `NATS_URL` is unset or the
    /// connection fails, so the system degrades gracefully to DB polling.
    pub async fn connect(url: &str) -> Self {
        match async_nats::connect(url).await {
            Ok(client) => {
                tracing::info!(url, "NATS connected");
                Self {
                    client: Some(client),
                }
            }
            Err(error) => {
                tracing::warn!(url, %error, "NATS unavailable; falling back to DB polling");
                Self { client: None }
            }
        }
    }

    /// Return a handle that can be cloned and sent across tasks.
    pub fn handle(&self) -> NatsHandle {
        NatsHandle {
            client: self.client.clone(),
        }
    }

    /// Publish a notification to a subject. Fire-and-forget; callers must
    /// not depend on delivery for correctness — the DB is authoritative.
    pub async fn publish(&self, subject: &str, payload: &[u8]) {
        if let Some(client) = self.client.clone() {
            if let Err(error) = client
                .publish(subject.to_string(), payload.to_vec().into())
                .await
            {
                tracing::warn!(subject, %error, "NATS publish failed");
            }
        }
    }

    /// Subscribe to a subject. Returns a stream of raw message payloads.
    /// When NATS is unavailable the stream never yields — callers must
    /// fall back to periodic DB polling.
    pub async fn subscribe(&self, subject: &str) -> Result<NatsSubscription, String> {
        let Some(client) = &self.client else {
            return Ok(NatsSubscription::empty());
        };
        let sub = client
            .subscribe(subject.to_string())
            .await
            .map_err(|e| format!("NATS subscribe failed: {e}"))?;
        Ok(NatsSubscription { inner: Some(sub) })
    }
}

/// Cloneable handle to a NATS connection. Cheap to clone — safe to pass
/// to worker tasks.
#[derive(Clone)]
pub struct NatsHandle {
    client: Option<Client>,
}

impl NatsHandle {
    pub async fn publish(&self, subject: &str, payload: &[u8]) {
        if let Some(client) = self.client.clone() {
            if let Err(error) = client
                .publish(subject.to_string(), payload.to_vec().into())
                .await
            {
                tracing::warn!(subject, %error, "NATS publish failed (via handle)");
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub async fn terminal_request(
        &self,
        subject: &str,
        request: &TerminalRequest,
    ) -> Result<TerminalResponse, String> {
        let client = self
            .client
            .clone()
            .ok_or_else(|| "pent-agent 终端通道不可用（NATS 未连接）".to_string())?;
        let payload =
            serde_json::to_vec(request).map_err(|error| format!("终端请求序列化失败: {error}"))?;
        let message = tokio::time::timeout(
            Duration::from_secs(5),
            client.request(subject.to_string(), payload.into()),
        )
        .await
        .map_err(|_| "pent-agent 终端请求超时".to_string())?
        .map_err(|error| format!("pent-agent 终端请求失败: {error}"))?;
        serde_json::from_slice(&message.payload)
            .map_err(|error| format!("终端响应解析失败: {error}"))
    }

    /// Execute a structured recipe tool on a pent-agent worker. This uses a
    /// request/reply inbox instead of running security binaries in gateway.
    pub async fn request_tool_execution(
        &self,
        request: &RemoteToolExecutionRequest,
    ) -> Result<RemoteToolExecutionResponse, String> {
        let client = self
            .client
            .clone()
            .ok_or_else(|| "pent-agent 执行通道不可用（NATS 未连接）".to_string())?;
        let payload = serde_json::to_vec(request)
            .map_err(|error| format!("工具执行请求序列化失败: {error}"))?;
        let privileged = requires_privileged_worker(&request.tool_name);
        let execute_subject = if privileged {
            subjects::PRIVILEGED_TOOL_EXECUTE
        } else {
            subjects::CONVERSATION_TOOL_EXECUTE
        };
        let ping_subject = if privileged {
            subjects::PRIVILEGED_TOOL_WORKER_PING
        } else {
            subjects::TOOL_WORKER_PING
        };
        self.probe_tool_worker(ping_subject).await?;
        let inbox = client.new_inbox();
        let mut replies = client
            .subscribe(inbox.clone())
            .await
            .map_err(|error| format!("工具执行响应订阅失败: {error}"))?;
        let mut heartbeats = client
            .subscribe(subjects::tool_heartbeat(&request.execution_id))
            .await
            .map_err(|error| format!("工具执行心跳订阅失败: {error}"))?;
        client
            .publish_with_reply(
                execute_subject.to_string(),
                inbox,
                payload.into(),
            )
            .await
            .map_err(|error| format!("工具执行请求发送失败: {error}"))?;
        let cancel_subject = subjects::tool_cancel(&request.execution_id);
        let mut cancellation =
            RemoteCancellationGuard(Some((client.clone(), cancel_subject.clone())));
        let deadline = tokio::time::sleep(Duration::from_secs(
            request.timeout_seconds.clamp(1, 3_600) + 30,
        ));
        tokio::pin!(deadline);
        let mut watchdog = tokio::time::interval(Duration::from_secs(1));
        let mut last_heartbeat = tokio::time::Instant::now();
        let message = loop {
            tokio::select! {
                message = replies.next() => {
                    break message.ok_or_else(|| "pent-agent 执行响应通道已关闭".to_string())?;
                }
                heartbeat = heartbeats.next() => {
                    if heartbeat.is_some() {
                        last_heartbeat = tokio::time::Instant::now();
                    }
                }
                _ = watchdog.tick() => {
                    if last_heartbeat.elapsed() > Duration::from_secs(EXECUTION_HEARTBEAT_GRACE_SECS) {
                        let _ = client.publish(cancel_subject.clone(), Vec::new().into()).await;
                        cancellation.disarm();
                        return Err("pent-agent 执行节点心跳丢失（WORKER_UNAVAILABLE）".into());
                    }
                }
                _ = &mut deadline => {
                    let _ = client.publish(cancel_subject.clone(), Vec::new().into()).await;
                    cancellation.disarm();
                    return Err("等待 pent-agent 执行工具超时，已发送取消请求".into());
                }
            }
        };
        cancellation.disarm();
        serde_json::from_slice(&message.payload)
            .map_err(|error| format!("工具执行响应解析失败: {error}"))
    }

    async fn probe_tool_worker(&self, ping_subject: &str) -> Result<(), String> {
        let client = self
            .client
            .clone()
            .ok_or_else(|| "pent-agent 执行通道不可用（NATS 未连接）".to_string())?;
        let inbox = client.new_inbox();
        let mut replies = client
            .subscribe(inbox.clone())
            .await
            .map_err(|error| format!("pent-agent 探测订阅失败: {error}"))?;
        client
            .publish_with_reply(
                ping_subject.to_string(),
                inbox,
                Vec::new().into(),
            )
            .await
            .map_err(|error| format!("pent-agent 探测请求失败: {error}"))?;
        tokio::time::timeout(
            Duration::from_secs(WORKER_PROBE_TIMEOUT_SECS),
            replies.next(),
        )
        .await
        .map_err(|_| "没有可用的 pent-agent 工具执行节点".to_string())?
        .ok_or_else(|| "pent-agent 探测响应通道已关闭".to_string())?;
        Ok(())
    }
}

/// Tools in this deliberately small allowlist are dispatched only to a worker
/// explicitly started with `PENTA_WORKER_PROFILE=privileged`.
pub fn requires_privileged_worker(tool_name: &str) -> bool {
    matches!(tool_name, "masscan" | "ssh_exec" | "sql_query")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRequest {
    pub action: String,
    pub session_id: Option<String>,
    pub data: Option<String>,
    pub after: Option<u64>,
    pub actor: Option<String>,
    pub tenant_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalChunk {
    pub sequence: u64,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalResponse {
    pub session_id: Option<String>,
    pub chunks: Vec<TerminalChunk>,
    pub sequence: u64,
    pub closed: bool,
    pub error: Option<String>,
}

impl TerminalResponse {
    fn error(error: impl Into<String>) -> Self {
        Self {
            session_id: None,
            chunks: Vec::new(),
            sequence: 0,
            closed: true,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteToolExecutionRequest {
    pub execution_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub scope_snapshot: serde_json::Value,
    pub session_id: String,
    pub run_id: Option<String>,
    /// Correlation identifier injected into proxy-aware HTTP clients without
    /// implying that this execution belongs to a `pent_runs` row.
    #[serde(default)]
    pub traffic_run_id: Option<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteToolExecutionResponse {
    pub output: Option<RemoteToolOutput>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub audit: kairos_platform_pent_tooling::backend::ExecutionAudit,
}

static CONVERSATION_TOOL_BUS: OnceLock<NatsHandle> = OnceLock::new();

pub fn init_conversation_tool_bus(handle: NatsHandle) -> Result<(), String> {
    if !handle.is_connected() {
        return Err("NATS 未连接，无法初始化 pent-agent 工具执行通道".into());
    }
    CONVERSATION_TOOL_BUS
        .set(handle)
        .map_err(|_| "pent-agent 工具执行通道已初始化".to_string())
}

pub fn conversation_tool_bus() -> Option<NatsHandle> {
    CONVERSATION_TOOL_BUS.get().cloned()
}

/// Start the queue-group responder hosted by pent-agent. Queue subscription
/// guarantees that exactly one worker handles each conversation tool call.
pub async fn spawn_conversation_tool_responder(
    bus: &NatsEventBus,
    pool: kairos_platform_framework_database::PgPool,
) -> Result<(), String> {
    let Some(client) = bus.client.clone() else {
        return Err("NATS 未连接，无法启动对话工具执行器".into());
    };
    let privileged_profile = std::env::var("PENTA_WORKER_PROFILE")
        .is_ok_and(|value| value.eq_ignore_ascii_case("privileged"));
    let (execute_subject, ping_subject, queue_group) = if privileged_profile {
        (
            subjects::PRIVILEGED_TOOL_EXECUTE,
            subjects::PRIVILEGED_TOOL_WORKER_PING,
            "penta-privileged-tool-workers",
        )
    } else {
        (
            subjects::CONVERSATION_TOOL_EXECUTE,
            subjects::TOOL_WORKER_PING,
            "penta-tool-workers",
        )
    };
    tracing::info!(
        worker_profile = if privileged_profile { "privileged" } else { "standard" },
        "pent-agent 工具执行 worker profile 已启用"
    );
    let mut requests = client
        .queue_subscribe(
            execute_subject.to_string(),
            queue_group.to_string(),
        )
        .await
        .map_err(|error| format!("订阅对话工具执行队列失败: {error}"))?;
    let mut probes = client
        .queue_subscribe(
            ping_subject.to_string(),
            queue_group.to_string(),
        )
        .await
        .map_err(|error| format!("订阅对话工具探测队列失败: {error}"))?;
    let probe_client = client.clone();
    tokio::spawn(async move {
        while let Some(message) = probes.next().await {
            if let Some(reply) = message.reply
                && let Err(error) = probe_client.publish(reply, "ready".into()).await
            {
                tracing::warn!(%error, "发送 pent-agent 探测响应失败");
            }
        }
    });
    tokio::spawn(async move {
        while let Some(message) = requests.next().await {
            let Some(reply) = message.reply else {
                tracing::warn!("忽略没有 reply subject 的工具执行请求");
                continue;
            };
            let response_client = client.clone();
            let response_pool = pool.clone();
            tokio::spawn(async move {
                let response =
                    execute_remote_tool_request(&response_client, &response_pool, &message.payload)
                        .await;
                let payload = serde_json::to_vec(&response).unwrap_or_else(|_| {
                    br#"{"output":null,"error":"response serialization failed"}"#.to_vec()
                });
                if let Err(error) = response_client.publish(reply, payload.into()).await {
                    tracing::warn!(%error, "发送对话工具执行响应失败");
                }
            });
        }
    });
    Ok(())
}

#[derive(Default)]
struct TerminalOutputBuffer {
    sequence: u64,
    chunks: Vec<TerminalChunk>,
}

/// Start the pent-agent-owned interactive terminal responder. Creation is
/// load-balanced; every created session then receives a unique NATS subject
/// subscribed only by the worker that owns its sandbox process.
pub async fn spawn_terminal_responder(
    bus: &NatsEventBus,
    pool: kairos_platform_framework_database::PgPool,
) -> Result<(), String> {
    let Some(client) = bus.client.clone() else {
        return Err("NATS 未连接，无法启动受治理终端执行器".into());
    };
    let mut creates = client
        .queue_subscribe(
            subjects::TERMINAL_CREATE.to_string(),
            "penta-terminal-workers".to_string(),
        )
        .await
        .map_err(|error| format!("订阅终端创建队列失败: {error}"))?;
    tokio::spawn(async move {
        while let Some(message) = creates.next().await {
            let Some(reply) = message.reply else { continue };
            let response_client = client.clone();
            let response_pool = pool.clone();
            tokio::spawn(async move {
                let request: Result<TerminalRequest, _> = serde_json::from_slice(&message.payload);
                let response = match request {
                    Ok(request) if request.action == "create" => {
                        create_terminal_session(response_client.clone(), response_pool, request)
                            .await
                    }
                    Ok(_) => TerminalResponse::error("终端创建请求 action 无效"),
                    Err(error) => TerminalResponse::error(format!("终端创建请求无效: {error}")),
                };
                let payload = serde_json::to_vec(&response).unwrap_or_default();
                let _ = response_client.publish(reply, payload.into()).await;
            });
        }
    });
    Ok(())
}

async fn create_terminal_session(
    client: Client,
    pool: kairos_platform_framework_database::PgPool,
    create_request: TerminalRequest,
) -> TerminalResponse {
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut sandbox =
        match crate::infrastructure::process::spawn_interactive_shell(&session_id, 3_600).await {
            Ok(sandbox) => sandbox,
            Err(error) => return TerminalResponse::error(error),
        };
    let mut input = match sandbox.child.stdin.take() {
        Some(input) => input,
        None => return TerminalResponse::error("受治理终端输入不可用"),
    };
    let Some(stdout) = sandbox.child.stdout.take() else {
        return TerminalResponse::error("受治理终端输出不可用");
    };
    let Some(stderr) = sandbox.child.stderr.take() else {
        return TerminalResponse::error("受治理终端错误输出不可用");
    };
    let subject = subjects::terminal_session(&session_id);
    let mut requests = match client.subscribe(subject).await {
        Ok(requests) => requests,
        Err(error) => return TerminalResponse::error(format!("订阅终端会话失败: {error}")),
    };
    let output = Arc::new(Mutex::new(TerminalOutputBuffer::default()));
    for (mut reader, buffer) in [
        (
            Box::new(stdout) as Box<dyn tokio::io::AsyncRead + Unpin + Send>,
            output.clone(),
        ),
        (
            Box::new(stderr) as Box<dyn tokio::io::AsyncRead + Unpin + Send>,
            output.clone(),
        ),
    ] {
        tokio::spawn(async move {
            let mut bytes = [0_u8; 8192];
            loop {
                let Ok(size) = reader.read(&mut bytes).await else {
                    break;
                };
                if size == 0 {
                    break;
                }
                let mut state = buffer.lock().await;
                state.sequence += 1;
                let sequence = state.sequence;
                state.chunks.push(TerminalChunk {
                    sequence,
                    data: String::from_utf8_lossy(&bytes[..size]).into_owned(),
                });
                if state.chunks.len() > 1_000 {
                    state.chunks.drain(..500);
                }
            }
        });
    }
    let response_session_id = session_id.clone();
    let actor = create_request.actor.unwrap_or_else(|| "unknown".into());
    let tenant_id = create_request.tenant_id.unwrap_or(0);
    if let Err(error) =
        persist_terminal_audit(&pool, &session_id, &actor, tenant_id, "created", None).await
    {
        let _ = sandbox.child.kill().await;
        return TerminalResponse::error(error);
    }
    tokio::spawn(async move {
        let expires = tokio::time::sleep(Duration::from_secs(3_600));
        tokio::pin!(expires);
        loop {
            tokio::select! {
                _ = &mut expires => {
                    let _ = sandbox.child.kill().await;
                    if let Err(error) = persist_terminal_audit(&pool, &session_id, &actor, tenant_id, "expired", None).await {
                        tracing::warn!(%error, session_id, "终端超时审计写入失败");
                    }
                    break;
                }
                message = requests.next() => {
                    let Some(message) = message else { break };
                    let Some(reply) = message.reply else { continue };
                    let request: Result<TerminalRequest, _> = serde_json::from_slice(&message.payload);
                    let mut response = TerminalResponse {
                        session_id: Some(session_id.clone()),
                        chunks: Vec::new(),
                        sequence: 0,
                        closed: false,
                        error: None,
                    };
                    match request {
                        Ok(request) if request.session_id.as_deref() != Some(&session_id) => {
                            response = TerminalResponse::error("终端 session_id 不匹配");
                        }
                        Ok(request) if request.action == "input" => {
                            let data = request.data.unwrap_or_default();
                            if data.len() > 64 * 1024 {
                                response = TerminalResponse::error("单次终端输入超过 64 KiB");
                            } else if let Err(error) = persist_terminal_audit(&pool, &session_id, &actor, tenant_id, "input", Some(&data)).await {
                                response = TerminalResponse::error(error);
                            } else if let Err(error) = input.write_all(data.as_bytes()).await {
                                response = TerminalResponse::error(format!("写入终端失败: {error}"));
                            } else {
                                let _ = input.flush().await;
                            }
                        }
                        Ok(request) if request.action == "output" => {
                            let after = request.after.unwrap_or(0);
                            let state = output.lock().await;
                            response.sequence = state.sequence;
                            response.chunks = state.chunks.iter().filter(|chunk| chunk.sequence > after).cloned().collect();
                            response.closed = sandbox.child.try_wait().ok().flatten().is_some();
                        }
                        Ok(request) if request.action == "close" => {
                            let _ = sandbox.child.kill().await;
                            if let Err(error) = persist_terminal_audit(&pool, &session_id, &actor, tenant_id, "closed", None).await {
                                tracing::warn!(%error, session_id, "终端关闭审计写入失败");
                                response.error = Some(error);
                            }
                            response.closed = true;
                        }
                        Ok(_) => response = TerminalResponse::error("终端 action 无效"),
                        Err(error) => response = TerminalResponse::error(format!("终端请求无效: {error}")),
                    }
                    let closed = response.closed;
                    let payload = serde_json::to_vec(&response).unwrap_or_default();
                    let _ = client.publish(reply, payload.into()).await;
                    if closed { break; }
                }
            }
        }
    });
    TerminalResponse {
        session_id: Some(response_session_id),
        chunks: Vec::new(),
        sequence: 0,
        closed: false,
        error: None,
    }
}

async fn persist_terminal_audit(
    pool: &kairos_platform_framework_database::PgPool,
    session_id: &str,
    actor: &str,
    tenant_id: i64,
    event: &str,
    input: Option<&str>,
) -> Result<(), String> {
    let input_metadata = input.map(terminal_input_metadata);
    let extra = serde_json::json!({
        "session_id": session_id,
        "event": event,
        "input": input_metadata,
        "network_policy": "none",
        "backend": "bubblewrap"
    })
    .to_string();
    sqlx::query(
        "INSERT INTO system_operate_log(
             id,trace_id,user_id,user_type,type,sub_type,biz_id,action,success,
             extra,request_method,request_url,creator,updater,tenant_id
         ) VALUES(nextval('system_operate_log_seq'),$1,0,0,'pent_terminal',$2,0,$3,true,
                  $4,'NATS','penta.terminal',$5,$5,$6)",
    )
    .bind(session_id)
    .bind(event)
    .bind(format!("terminal.{event}"))
    .bind(extra)
    .bind(actor.chars().take(64).collect::<String>())
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(|error| format!("持久化终端审计失败: {error}"))?;
    Ok(())
}

fn terminal_input_metadata(value: &str) -> serde_json::Value {
    serde_json::json!({
        "bytes": value.len(),
        "sha256": hex::encode(Sha256::digest(value.as_bytes()))
    })
}

async fn execute_remote_tool_request(
    client: &Client,
    pool: &kairos_platform_framework_database::PgPool,
    payload: &[u8],
) -> RemoteToolExecutionResponse {
    let request: RemoteToolExecutionRequest = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(error) => {
            return RemoteToolExecutionResponse {
                output: None,
                error: Some(format!("工具执行请求解析失败: {error}")),
            };
        }
    };
    if !valid_execution_id(&request.execution_id) {
        return RemoteToolExecutionResponse {
            output: None,
            error: Some("工具 execution_id 格式无效".into()),
        };
    }
    let heartbeat_client = client.clone();
    let heartbeat_subject = subjects::tool_heartbeat(&request.execution_id);
    let _heartbeat = AbortTask(tokio::spawn(async move {
        loop {
            if heartbeat_client
                .publish(heartbeat_subject.clone(), Vec::new().into())
                .await
                .is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_secs(EXECUTION_HEARTBEAT_INTERVAL_SECS)).await;
        }
    }));
    let Some((target_type, profile)) = remote_tool_policy(&request.tool_name) else {
        return RemoteToolExecutionResponse {
            output: None,
            error: Some(format!(
                "工具 {} 未在 pent-agent 配方中注册",
                request.tool_name
            )),
        };
    };
    let privileged_profile = std::env::var("PENTA_WORKER_PROFILE")
        .is_ok_and(|value| value.eq_ignore_ascii_case("privileged"));
    if requires_privileged_worker(&request.tool_name) != privileged_profile {
        return RemoteToolExecutionResponse {
            output: None,
            error: Some("工具与 pent-agent worker profile 不匹配，拒绝执行".into()),
        };
    }
    if target_type == crate::infrastructure::recipe::TargetType::Network
        && request.scope_snapshot.get("confirmed_at").is_none()
        && request
            .scope_snapshot
            .get("confirmed")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
    {
        return RemoteToolExecutionResponse {
            output: None,
            error: Some("目标范围尚未人工确认，pent-agent 拒绝执行网络工具".into()),
        };
    }
    let mut arguments = request.arguments;
    let expected_provider = arguments
        .get("provider")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let credential_required = crate::infrastructure::recipe::lookup(&request.tool_name)
        .is_some_and(|recipe| {
            recipe.parameters.iter().any(|parameter| {
                parameter.runtime_only && parameter.required && parameter.name == "_credential_ref"
            })
        });
    let credential_ref = arguments
        .as_object_mut()
        .and_then(|object| object.remove("_credential_ref"))
        .and_then(|value| value.as_str().map(str::to_owned));
    if credential_required && credential_ref.is_none() {
        return RemoteToolExecutionResponse {
            output: None,
            error: Some("工具要求租户绑定的 _credential_ref".into()),
        };
    }
    let secret_environment = match credential_ref {
        Some(reference) => match crate::infrastructure::cloud_credentials::resolve_for_execution(
            pool,
            &reference,
            &request.tool_name,
            expected_provider.as_deref(),
            request.run_id.as_deref(),
            &request.session_id,
        )
        .await
        {
            Ok(environment) => environment,
            Err(error) => {
                return RemoteToolExecutionResponse {
                    output: None,
                    error: Some(error),
                };
            }
        },
        None => Default::default(),
    };
    let artifact_run_id = request.run_id.clone();
    let http_request = if request.tool_name == "http_request" {
        match prepare_remote_http_request(&request.session_id, &mut arguments).await {
            Ok(prepared) => Some(prepared),
            Err(error) => {
                return RemoteToolExecutionResponse {
                    output: None,
                    error: Some(error),
                };
            }
        }
    } else {
        None
    };
    let mut limits = ToolExecutionLimits {
        cpu_cores: profile.cpu_cores.clamp(0.1, 16.0),
        memory_mb: profile.memory_mb.clamp(32, 32_768),
        timeout_seconds: profile.timeout_seconds.clamp(1, 3_600),
        network_access: NetworkPolicy::derive_from_scope(&request.scope_snapshot),
        run_id: request.traffic_run_id.or(request.run_id),
        secret_environment,
        ..Default::default()
    };
    if let Some((_, csrf_path)) = &http_request
        && let Some(state_dir) = csrf_path.parent()
    {
        limits
            .allowed_paths
            .push(state_dir.to_string_lossy().into_owned());
    }
    let cancel_subject = subjects::tool_cancel(&request.execution_id);
    let mut cancellations = match client.subscribe(cancel_subject).await {
        Ok(subscription) => subscription,
        Err(error) => {
            return RemoteToolExecutionResponse {
                output: None,
                error: Some(format!("订阅工具取消通道失败: {error}")),
            };
        }
    };
    let execution = async {
        if let Some((command, _)) = &http_request {
            crate::infrastructure::process::execute_tool("http_request", command, &limits).await
        } else {
            crate::infrastructure::process::execute_tool_json(
                &request.tool_name,
                &arguments,
                &limits,
            )
            .await
        }
    };
    let result = tokio::select! {
        result = execution => result,
        _ = cancellations.next() => Err("工具执行已取消".to_string()),
    };
    match result {
        Ok(output) => {
            if let Some((_, csrf_path)) = &http_request
                && let Some(token) =
                    crate::conversation::tool_executor::extract_csrf_token(&output.stdout)
            {
                let _ = tokio::fs::write(csrf_path, token).await;
            }
            let output = RemoteToolOutput {
                stdout: truncate_utf8(output.stdout, MAX_REMOTE_OUTPUT_BYTES),
                stderr: truncate_utf8(output.stderr, MAX_REMOTE_OUTPUT_BYTES),
                exit_code: output.exit_code,
                duration_ms: output.duration_ms,
                audit: output.audit,
            };
            if let Some(run_id) = artifact_run_id.as_deref()
                && let Err(error) = persist_remote_tool_call(
                    pool,
                    run_id,
                    &request.execution_id,
                    &request.tool_name,
                    &arguments,
                    &output,
                )
                .await
            {
                return RemoteToolExecutionResponse {
                    output: None,
                    error: Some(format!("持久化工具执行审计失败: {error}")),
                };
            }
            if let Some(run_id) = artifact_run_id.filter(|run_id| valid_execution_id(run_id)) {
                let artifact_pool = pool.clone();
                let execution_id = request.execution_id.clone();
                let artifact_output = output.clone();
                tokio::spawn(async move {
                    if let Err(error) = persist_execution_artifact(
                        &artifact_pool,
                        &run_id,
                        &execution_id,
                        &artifact_output,
                    )
                    .await
                    {
                        tracing::warn!(%error, run_id, "持久化对话工具执行产物失败");
                    }
                });
            }
            RemoteToolExecutionResponse {
                output: Some(output),
                error: None,
            }
        }
        Err(error) => {
            if let Some(run_id) = artifact_run_id.as_deref()
                && let Err(persist_error) = persist_remote_tool_failure(
                    pool,
                    run_id,
                    &request.execution_id,
                    &request.tool_name,
                    &arguments,
                    &error,
                )
                .await
            {
                return RemoteToolExecutionResponse {
                    output: None,
                    error: Some(format!("{error}; 持久化工具执行审计失败: {persist_error}")),
                };
            }
            if let Some(run_id) = artifact_run_id.filter(|run_id| valid_execution_id(run_id)) {
                let artifact_pool = pool.clone();
                let execution_id = request.execution_id.clone();
                let tool_name = request.tool_name.clone();
                let artifact_error = error.clone();
                tokio::spawn(async move {
                    let payload =
                        failure_artifact_payload(&execution_id, &tool_name, &artifact_error);
                    if let Err(persist_error) =
                        persist_json_artifact(&artifact_pool, &run_id, &execution_id, &payload)
                            .await
                    {
                        tracing::warn!(%persist_error, run_id, "持久化对话工具失败产物失败");
                    }
                });
            }
            RemoteToolExecutionResponse {
                output: None,
                error: Some(error),
            }
        }
    }
}

fn remote_tool_policy(
    tool_name: &str,
) -> Option<(
    crate::infrastructure::recipe::TargetType,
    crate::infrastructure::recipe::RecipeResourceProfile,
)> {
    if tool_name == "http_request" {
        return Some((
            crate::infrastructure::recipe::TargetType::Network,
            crate::infrastructure::recipe::RecipeResourceProfile {
                cpu_cores: 0.1,
                memory_mb: 32,
                timeout_seconds: 30,
            },
        ));
    }
    crate::infrastructure::recipe::lookup(tool_name)
        .map(|recipe| (recipe.target_type, recipe.resource_profile))
}

async fn persist_remote_tool_call(
    pool: &kairos_platform_framework_database::PgPool,
    run_id: &str,
    execution_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    output: &RemoteToolOutput,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pent_tool_calls(
             id,run_id,tool_name,params_summary,permission_decision,
             started_at,completed_at,duration_ms,exit_code,executor_backend,
             network_policy,execution_audit
         ) VALUES($1,$2,$3,$4,'allowed',now(),now(),$5,$6,$7,$8,$9)
         ON CONFLICT(id) DO UPDATE SET
             completed_at=now(),duration_ms=excluded.duration_ms,
             exit_code=excluded.exit_code,executor_backend=excluded.executor_backend,
             network_policy=excluded.network_policy,execution_audit=excluded.execution_audit",
    )
    .bind(execution_id)
    .bind(run_id)
    .bind(tool_name)
    .bind(redact_remote_arguments(arguments.clone()))
    .bind(i64::try_from(output.duration_ms).unwrap_or(i64::MAX))
    .bind(output.exit_code)
    .bind(&output.audit.backend)
    .bind(&output.audit.network_policy)
    .bind(serde_json::to_value(&output.audit).unwrap_or_else(|_| serde_json::json!({})))
    .execute(pool)
    .await?;
    Ok(())
}

async fn persist_remote_tool_failure(
    pool: &kairos_platform_framework_database::PgPool,
    run_id: &str,
    execution_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pent_tool_calls(
             id,run_id,tool_name,params_summary,permission_decision,
             started_at,completed_at,duration_ms,exit_code,executor_backend,execution_audit
         ) VALUES($1,$2,$3,$4,'allowed',now(),now(),0,-1,'pent-agent',$5)
         ON CONFLICT(id) DO UPDATE SET completed_at=now(),exit_code=-1,
             executor_backend='pent-agent',execution_audit=excluded.execution_audit",
    )
    .bind(execution_id)
    .bind(run_id)
    .bind(tool_name)
    .bind(redact_remote_arguments(arguments.clone()))
    .bind(serde_json::json!({
        "backend": "pent-agent",
        "outcome": "failed",
        "error": error,
    }))
    .execute(pool)
    .await?;
    Ok(())
}

fn redact_remote_arguments(mut value: serde_json::Value) -> serde_json::Value {
    match &mut value {
        serde_json::Value::Object(object) => {
            for (key, item) in object {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "authorization"
                        | "cookie"
                        | "set-cookie"
                        | "password"
                        | "token"
                        | "csrf_token"
                        | "api_key"
                        | "_credential_ref"
                ) {
                    *item = serde_json::Value::String("[REDACTED]".into());
                } else {
                    *item = redact_remote_arguments(item.take());
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                *item = redact_remote_arguments(item.take());
            }
        }
        _ => {}
    }
    value
}

async fn persist_execution_artifact(
    pool: &kairos_platform_framework_database::PgPool,
    run_id: &str,
    execution_id: &str,
    output: &RemoteToolOutput,
) -> Result<(), String> {
    let value = serde_json::to_value(output).map_err(|error| error.to_string())?;
    persist_json_artifact(pool, run_id, execution_id, &value).await
}

async fn prepare_remote_http_request(
    session_id: &str,
    arguments: &mut serde_json::Value,
) -> Result<(Vec<String>, std::path::PathBuf), String> {
    let url = arguments
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "http_request 缺少 url".to_string())?
        .to_owned();
    let method = arguments
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let body = arguments
        .get("body")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut headers = arguments
        .get("headers")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let state_dir = crate::conversation::tool_executor::http_session_state_dir(session_id).await?;
    let csrf_path = state_dir.join("csrf-token");
    if let Ok(token) = tokio::fs::read_to_string(&csrf_path).await {
        let header_name = arguments
            .get("csrf_header")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("X-CSRF-Token");
        if !headers
            .keys()
            .any(|name| name.eq_ignore_ascii_case(header_name))
        {
            headers.insert(header_name.into(), token.into());
        }
    }
    let form = arguments
        .get("form")
        .filter(|value| value.is_object())
        .map(serde_json::Value::to_string)
        .unwrap_or_default();
    Ok((
        vec![
            url,
            method,
            body,
            serde_json::Value::Object(headers).to_string(),
            form,
        ],
        csrf_path,
    ))
}

fn failure_artifact_payload(execution_id: &str, tool_name: &str, error: &str) -> serde_json::Value {
    let outcome = if error.contains("取消") {
        "cancelled"
    } else if error.contains("超时") {
        "timed_out"
    } else {
        "failed"
    };
    serde_json::json!({
        "schema_version": 1,
        "execution_id": execution_id,
        "tool_name": tool_name,
        "outcome": outcome,
        "error": error,
        "executor_backend": "bubblewrap"
    })
}

async fn persist_json_artifact(
    pool: &kairos_platform_framework_database::PgPool,
    run_id: &str,
    execution_id: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let object_key = format!("runs/{run_id}/conversation-tools/{execution_id}.json");
    crate::infrastructure::object_store::ObjectStore::from_env()
        .put_object(&object_key, bytes.clone(), "application/json")
        .await?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    sqlx::query(
        "INSERT INTO pent_artifacts(
             id,run_id,object_key,size_bytes,sha256,content_type,created_at
         ) VALUES($1,$2,$3,$4,$5,'application/json',now())
         ON CONFLICT DO NOTHING",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(object_key)
    .bind(i64::try_from(bytes.len()).unwrap_or(i64::MAX))
    .bind(sha256)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn truncate_utf8(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push_str("\n[output truncated by remote executor]");
    value
}

fn valid_execution_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

/// NATS subscription — yields message payloads or is empty when NATS is
/// unavailable.
pub struct NatsSubscription {
    inner: Option<async_nats::Subscriber>,
}

impl NatsSubscription {
    fn empty() -> Self {
        Self { inner: None }
    }

    /// Wait for the next message. Returns `None` when the subscription is
    /// dropped or NATS is unavailable (callers fall back to polling).
    pub async fn next(&mut self) -> Option<Vec<u8>> {
        match &mut self.inner {
            Some(sub) => sub.next().await.map(|msg| msg.payload.to_vec()),
            None => {
                // NATS unavailable — never yield; callers must use a
                // tokio::time::interval for DB polling instead.
                std::future::pending::<()>().await;
                unreachable!()
            }
        }
    }
}

/// Pent task subjects organized by queue tier.
pub mod subjects {
    /// Queue-tiered run request subjects. Workers subscribe to the tier they
    /// service; the API publishes to the tier matching the run's workload.
    pub mod queue {
        pub const SCAN_LIGHT: &str = "penta.scan.light.requested";
        pub const SCAN_HEAVY: &str = "penta.scan.heavy.requested";
        pub const NATIVE_FAST: &str = "penta.native.fast.requested";
        pub const BROWSER: &str = "penta.browser.requested";
        pub const DANGEROUS: &str = "penta.dangerous.requested";
        pub const REPORT: &str = "penta.report.requested";
    }

    /// Run lifecycle subjects.
    pub const RUN_REQUESTED: &str = "penta.run.requested";
    pub const RUN_CANCELLED: &str = "penta.run.cancelled";

    /// Approval workflow subjects.
    pub const APPROVAL_DECIDED: &str = "penta.approval.decided";

    /// Evidence created (for downstream consumers).
    pub const EVIDENCE_CREATED: &str = "penta.evidence.created";

    /// Worker heartbeat for liveness monitoring.
    pub const WORKER_HEARTBEAT: &str = "penta.worker.heartbeat";

    /// Request/reply endpoint for conversation recipe tools. Only pent-agent
    /// workers subscribe to this subject.
    pub const CONVERSATION_TOOL_EXECUTE: &str = "penta.tool.execute";
    pub const TOOL_WORKER_PING: &str = "penta.tool.worker.ping";
    pub const PRIVILEGED_TOOL_EXECUTE: &str = "penta.tool.privileged.execute";
    pub const PRIVILEGED_TOOL_WORKER_PING: &str = "penta.tool.privileged.worker.ping";
    pub const TERMINAL_CREATE: &str = "penta.terminal.create";

    pub fn tool_cancel(execution_id: &str) -> String {
        format!("penta.tool.cancel.{execution_id}")
    }

    pub fn tool_heartbeat(execution_id: &str) -> String {
        format!("penta.tool.heartbeat.{execution_id}")
    }

    pub fn terminal_session(session_id: &str) -> String {
        format!("penta.terminal.session.{session_id}")
    }

    /// Map a queue tier string (e.g. "scan.light") to its subject.
    pub fn run_requested_for_tier(tier: &str) -> &'static str {
        match tier {
            "scan.light" => queue::SCAN_LIGHT,
            "scan.heavy" => queue::SCAN_HEAVY,
            "native.fast" => queue::NATIVE_FAST,
            "browser" => queue::BROWSER,
            "dangerous" => queue::DANGEROUS,
            "report" => queue::REPORT,
            _ => {
                tracing::warn!(tier, "unknown queue tier; defaulting to scan.light");
                queue::SCAN_LIGHT
            }
        }
    }
}

/// Classify a tool into a queue tier. Mirrors
/// [`kairos_platform_pent_tooling::dispatcher::dispatch_queue`].
/// but operates on tool names directly rather than risk/tool_type pairs.
pub fn classify_tool_queue(tool_name: &str) -> &'static str {
    match tool_name {
        // Light info gathering
        "http_probe" | "dns_lookup" | "web_crawl" | "web_fingerprint" | "web_url_collect"
        | "js_collect" | "knowledge_search" | "evidence_list" | "evidence_search"
        | "evidence_view" | "traffic_list" | "traffic_view" | "traffic_diff" => "scan.light",
        // Heavier active scanning
        "http_request" | "dir_fuzz" | "js_analyze" | "file_upload_validate" => "scan.light",
        // Heavy scanning
        "tcp_scan" | "scanner_nmap" | "scanner_nuclei" | "traffic_replay" => "scan.heavy",
        _ => "scan.light",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privileged_tools_use_a_dedicated_worker_profile() {
        for tool in ["masscan", "ssh_exec", "sql_query"] {
            assert!(requires_privileged_worker(tool));
        }
        assert!(!requires_privileged_worker("nuclei"));
        assert!(!requires_privileged_worker("nmap"));
    }

    #[test]
    fn builtin_http_request_has_remote_execution_policy() {
        let (target_type, profile) = remote_tool_policy("http_request").expect("builtin policy");
        assert_eq!(target_type, crate::infrastructure::recipe::TargetType::Network);
        assert_eq!(profile.timeout_seconds, 30);
    }

    #[test]
    fn remote_request_round_trips() {
        let request = RemoteToolExecutionRequest {
            execution_id: "execution-1".into(),
            tool_name: "nmap".into(),
            arguments: serde_json::json!({"target": "example.test"}),
            scope_snapshot: serde_json::json!({"targets": ["example.test"]}),
            session_id: "session-1".into(),
            run_id: Some("run-1".into()),
            traffic_run_id: None,
            timeout_seconds: 300,
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        let decoded: RemoteToolExecutionRequest = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.tool_name, "nmap");
        assert_eq!(decoded.arguments["target"], "example.test");
        assert_eq!(decoded.scope_snapshot["targets"][0], "example.test");
        assert_eq!(decoded.run_id.as_deref(), Some("run-1"));
    }

    #[test]
    fn remote_output_truncation_preserves_utf8() {
        let truncated = truncate_utf8("测".repeat(10), 10);
        assert!(truncated.starts_with("测测测"));
        assert!(truncated.ends_with("[output truncated by remote executor]"));
    }

    #[test]
    fn execution_ids_are_safe_for_nats_subjects() {
        assert!(valid_execution_id("a4d15c16-68a5-489a-a4af-8d49f98d99de"));
        assert!(!valid_execution_id("unsafe.>"));
        assert!(!valid_execution_id(""));
        assert_eq!(
            subjects::tool_heartbeat("execution-1"),
            "penta.tool.heartbeat.execution-1"
        );
        assert!(EXECUTION_HEARTBEAT_GRACE_SECS > EXECUTION_HEARTBEAT_INTERVAL_SECS * 2);
        assert!(EXECUTION_HEARTBEAT_GRACE_SECS <= 10);
    }

    #[test]
    fn terminal_protocol_round_trips_and_uses_session_affinity() {
        let request = TerminalRequest {
            action: "input".into(),
            session_id: Some("safe-session-1".into()),
            data: Some("pwd\n".into()),
            after: None,
            actor: Some("user-1".into()),
            tenant_id: Some(1),
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        let decoded: TerminalRequest = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.session_id.as_deref(), Some("safe-session-1"));
        assert_eq!(
            subjects::terminal_session("safe-session-1"),
            "penta.terminal.session.safe-session-1"
        );
    }

    #[test]
    fn terminal_audit_metadata_never_contains_raw_input() {
        let secret = "export TOKEN=super-secret-value\n";
        let metadata = terminal_input_metadata(secret);
        assert_eq!(metadata["bytes"], secret.len());
        assert_eq!(metadata["sha256"].as_str().unwrap().len(), 64);
        assert!(!metadata.to_string().contains("super-secret-value"));
    }

    #[test]
    fn failure_artifacts_are_minimal_and_classified() {
        let cancelled = failure_artifact_payload("exec-1", "nmap", "工具执行已取消");
        assert_eq!(cancelled["outcome"], "cancelled");
        assert_eq!(cancelled["tool_name"], "nmap");
        assert!(cancelled.get("arguments").is_none());
        assert!(cancelled.get("credentials").is_none());

        let timed_out = failure_artifact_payload("exec-2", "nuclei", "工具 nuclei 执行超时");
        assert_eq!(timed_out["outcome"], "timed_out");
        let failed = failure_artifact_payload("exec-3", "afrog", "binary missing");
        assert_eq!(failed["outcome"], "failed");
    }

    #[test]
    fn remote_audit_arguments_are_recursively_redacted() {
        let redacted = redact_remote_arguments(serde_json::json!({
            "target": "example.test",
            "headers": {"Authorization": "Bearer secret", "x-test": "visible"},
            "items": [{"password": "secret"}],
            "_credential_ref": "tenant-secret"
        }));
        assert_eq!(redacted["target"], "example.test");
        assert_eq!(redacted["headers"]["Authorization"], "[REDACTED]");
        assert_eq!(redacted["headers"]["x-test"], "visible");
        assert_eq!(redacted["items"][0]["password"], "[REDACTED]");
        assert_eq!(redacted["_credential_ref"], "[REDACTED]");
    }

    #[tokio::test]
    async fn remote_http_request_keeps_csrf_state_on_agent() {
        let session_id = format!("http-state-test-{}", uuid::Uuid::new_v4());
        let mut arguments = serde_json::json!({
            "url":"https://example.test/login",
            "method":"POST",
            "body":"user=test",
            "headers":{"Content-Type":"application/x-www-form-urlencoded"}
        });
        let (command, csrf_path) = prepare_remote_http_request(&session_id, &mut arguments)
            .await
            .unwrap();
        assert_eq!(command[0], "https://example.test/login");
        assert_eq!(command[1], "POST");
        tokio::fs::write(&csrf_path, "csrf-test-token")
            .await
            .unwrap();

        let (command, _) = prepare_remote_http_request(&session_id, &mut arguments)
            .await
            .unwrap();
        assert!(command[3].contains("csrf-test-token"));
        if let Some(directory) = csrf_path.parent() {
            let _ = tokio::fs::remove_dir_all(directory).await;
        }
    }

    #[tokio::test]
    async fn disconnected_bus_fails_without_waiting() {
        let bus = NatsHandle { client: None };
        let request = RemoteToolExecutionRequest {
            execution_id: "execution-1".into(),
            tool_name: "nmap".into(),
            arguments: serde_json::json!({"target": "example.test"}),
            scope_snapshot: serde_json::json!({"targets": ["example.test"]}),
            session_id: "session-1".into(),
            run_id: None,
            traffic_run_id: None,
            timeout_seconds: 300,
        };
        let error = bus.request_tool_execution(&request).await.unwrap_err();
        assert!(error.contains("NATS 未连接"));
    }
}
