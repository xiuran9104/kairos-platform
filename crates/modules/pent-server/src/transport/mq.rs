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

use std::{sync::OnceLock, time::Duration};

use async_nats::Client;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::infrastructure::process::{NetworkPolicy, ToolExecutionLimits};

const WORKER_PROBE_TIMEOUT_SECS: u64 = 2;
const MAX_REMOTE_OUTPUT_BYTES: usize = 400 * 1024;

struct RemoteCancellationGuard(Option<(Client, String)>);

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
        self.probe_tool_worker().await?;
        let inbox = client.new_inbox();
        let mut replies = client
            .subscribe(inbox.clone())
            .await
            .map_err(|error| format!("工具执行响应订阅失败: {error}"))?;
        client
            .publish_with_reply(
                subjects::CONVERSATION_TOOL_EXECUTE.to_string(),
                inbox,
                payload.into(),
            )
            .await
            .map_err(|error| format!("工具执行请求发送失败: {error}"))?;
        let cancel_subject = subjects::tool_cancel(&request.execution_id);
        let mut cancellation =
            RemoteCancellationGuard(Some((client.clone(), cancel_subject.clone())));
        let message = match tokio::time::timeout(
            Duration::from_secs(request.timeout_seconds.clamp(1, 3_600) + 30),
            replies.next(),
        )
        .await
        {
            Ok(message) => {
                message.ok_or_else(|| "pent-agent 执行响应通道已关闭".to_string())?
            }
            Err(_) => {
                let _ = client.publish(cancel_subject, Vec::new().into()).await;
                cancellation.disarm();
                return Err("等待 pent-agent 执行工具超时，已发送取消请求".into());
            }
        };
        cancellation.disarm();
        serde_json::from_slice(&message.payload)
            .map_err(|error| format!("工具执行响应解析失败: {error}"))
    }

    async fn probe_tool_worker(&self) -> Result<(), String> {
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
                subjects::TOOL_WORKER_PING.to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteToolExecutionRequest {
    pub execution_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub scope_snapshot: serde_json::Value,
    pub session_id: String,
    pub run_id: Option<String>,
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
    let mut requests = client
        .queue_subscribe(
            subjects::CONVERSATION_TOOL_EXECUTE.to_string(),
            "penta-tool-workers".to_string(),
        )
        .await
        .map_err(|error| format!("订阅对话工具执行队列失败: {error}"))?;
    let mut probes = client
        .queue_subscribe(
            subjects::TOOL_WORKER_PING.to_string(),
            "penta-tool-workers".to_string(),
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
                let response = execute_remote_tool_request(
                    &response_client,
                    &response_pool,
                    &message.payload,
                )
                .await;
                let payload = serde_json::to_vec(&response).unwrap_or_else(|_| {
                    br#"{"output":null,"error":"响应序列化失败"}"#.to_vec()
                });
                if let Err(error) = response_client.publish(reply, payload.into()).await {
                    tracing::warn!(%error, "发送对话工具执行响应失败");
                }
            });
        }
    });
    Ok(())
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
    let Some(recipe) = crate::infrastructure::recipe::lookup(&request.tool_name) else {
        return RemoteToolExecutionResponse {
            output: None,
            error: Some(format!("工具 {} 未在 pent-agent 配方中注册", request.tool_name)),
        };
    };
    if recipe.target_type == crate::infrastructure::recipe::TargetType::Network
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
    let profile = recipe.resource_profile;
    let artifact_run_id = request.run_id.clone();
    let limits = ToolExecutionLimits {
        cpu_cores: profile.cpu_cores.clamp(0.1, 16.0),
        memory_mb: profile.memory_mb.clamp(32, 32_768),
        timeout_seconds: profile.timeout_seconds.clamp(1, 3_600),
        network_access: NetworkPolicy::derive_from_scope(&request.scope_snapshot),
        run_id: request.run_id,
        ..Default::default()
    };
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
    let result = tokio::select! {
        result = crate::infrastructure::process::execute_tool_json(
            &request.tool_name,
            &request.arguments,
            &limits,
        ) => result,
        _ = cancellations.next() => Err("工具执行已取消".to_string()),
    };
    match result {
        Ok(output) => {
            let output = RemoteToolOutput {
                stdout: truncate_utf8(output.stdout, MAX_REMOTE_OUTPUT_BYTES),
                stderr: truncate_utf8(output.stderr, MAX_REMOTE_OUTPUT_BYTES),
                exit_code: output.exit_code,
                duration_ms: output.duration_ms,
                audit: output.audit,
            };
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
        Err(error) => RemoteToolExecutionResponse {
            output: None,
            error: Some(error),
        },
    }
}

async fn persist_execution_artifact(
    pool: &kairos_platform_framework_database::PgPool,
    run_id: &str,
    execution_id: &str,
    output: &RemoteToolOutput,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(output).map_err(|error| error.to_string())?;
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

    pub fn tool_cancel(execution_id: &str) -> String {
        format!("penta.tool.cancel.{execution_id}")
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
    fn remote_request_round_trips() {
        let request = RemoteToolExecutionRequest {
            execution_id: "execution-1".into(),
            tool_name: "nmap".into(),
            arguments: serde_json::json!({"target": "example.test"}),
            scope_snapshot: serde_json::json!({"targets": ["example.test"]}),
            session_id: "session-1".into(),
            run_id: Some("run-1".into()),
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
            timeout_seconds: 300,
        };
        let error = bus.request_tool_execution(&request).await.unwrap_err();
        assert!(error.contains("NATS 未连接"));
    }
}
