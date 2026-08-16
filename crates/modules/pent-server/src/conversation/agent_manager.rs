//! Agent manager for concurrent sub-agent execution.
//!
//! Supports model-driven task decomposition: the model can call `agent_spawn`
//! to create sub-agents that run independent tool loops concurrently, then
//! `agent_collect` to gather results.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::Utc;
use kairos_platform_pent_api::{AgentMessage, AgentTurnRequest, ConversationMessageType};

use super::{
    ConversationContextSource, ConversationEventSink, ConversationIntentDetector,
    ConversationKnowledgeSource, ConversationModelGateway, ConversationRepository,
    ConversationToolExecutor, RuntimeToolCall, ToolApprovalGate, ToolApprovalOutcome,
    ToolApprovalRequest, ToolExecutionContext,
};
use kairos_platform_pent_runtime::agent_loop::{
    AgentDirective, AgentLoopEngine, AgentLoopLimits, AgentLoopStop, parse_agent_directive,
};
use kairos_platform_pent_runtime::ports::{
    AgentSession, AgentSessionFactory, AgentSpawnRequest, RuntimeAgentStatus, RuntimeSubAgent,
};

const SUB_AGENT_MAX_TOOL_ROUNDS: usize = 12;
const SUB_AGENT_MAX_REQUESTS: usize = 80;
const SUB_AGENT_STALL_LIMIT: usize = 4;
/// Sub-agents must produce at least one effective tool observation before FINAL.
const SUB_AGENT_MIN_TOOL_CALLS: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SubAgent {
    pub agent_id: String,
    pub description: String,
    pub status: AgentStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

impl SubAgent {
    fn new(agent_id: String, description: String) -> Self {
        Self {
            agent_id,
            description,
            status: AgentStatus::Running,
            result: None,
            error: None,
            started_at: Utc::now().to_rfc3339(),
            completed_at: None,
        }
    }
}

pub struct AgentManager {
    agents: Arc<Mutex<HashMap<String, SubAgent>>>,
    max_concurrent: usize,
}

impl AgentManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent,
        }
    }

    pub async fn active_count(&self) -> usize {
        self.agents
            .lock()
            .await
            .values()
            .filter(|a| a.status == AgentStatus::Running)
            .count()
    }

    pub async fn spawn(
        &self,
        description: String,
        prompt: String,
        session_id: String,
        parent_run_id: String,
        target: Option<String>,
        repository: Arc<dyn ConversationRepository>,
        gateway: Arc<dyn ConversationModelGateway>,
        context_source: Arc<dyn ConversationContextSource>,
        knowledge_source: Arc<dyn ConversationKnowledgeSource>,
        tool_executor: Arc<dyn ConversationToolExecutor>,
        approval_gate: Arc<dyn ToolApprovalGate>,
        events: Arc<dyn ConversationEventSink>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
        pool: kairos_platform_framework_database::PgPool,
    ) -> anyhow::Result<String> {
        let active = self.active_count().await;
        if active >= self.max_concurrent {
            anyhow::bail!(
                "已达到最大并发子代理数 ({})，请等待部分完成后重试",
                self.max_concurrent
            );
        }

        let agent_id = format!(
            "sub-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("unknown")
        );
        let store = self.agents.clone();

        {
            let mut agents = store.lock().await;
            agents.insert(
                agent_id.clone(),
                SubAgent::new(agent_id.clone(), description.clone()),
            );
        }

        repository
            .record_run_event(
                &parent_run_id,
                "agent.spawned",
                &serde_json::json!({
                    "agent_id": agent_id,
                    "description": description,
                    "target": target,
                    "status": "running"
                }),
            )
            .await?;

        let agent_id_clone = agent_id.clone();
        let store_clone = store.clone();

        tokio::spawn(async move {
            let event_repository = repository.clone();
            let event_run_id = parent_run_id.clone();
            let result = execute_sub_agent(
                &agent_id_clone,
                &session_id,
                &prompt,
                target.as_deref(),
                &description,
                repository,
                gateway,
                context_source,
                knowledge_source,
                tool_executor,
                approval_gate,
                events,
                intent_detector,
                pool,
            )
            .await;

            let mut agents = store_clone.lock().await;
            if let Some(agent) = agents.get_mut(&agent_id_clone) {
                match result {
                    Ok(content) => {
                        agent.status = AgentStatus::Completed;
                        agent.result = Some(content);
                    }
                    Err(error) => {
                        agent.status = AgentStatus::Failed;
                        agent.error = Some(error.to_string());
                    }
                }
                agent.completed_at = Some(Utc::now().to_rfc3339());
                let _ = event_repository
                    .record_run_event(
                        &event_run_id,
                        "agent.completed",
                        &serde_json::json!({
                            "agent_id": agent.agent_id,
                            "status": agent.status,
                            "result": agent.result,
                            "error": agent.error,
                            "completed_at": agent.completed_at
                        }),
                    )
                    .await;
            }
        });

        Ok(agent_id)
    }

    pub async fn status(&self, agent_id: &str) -> Option<SubAgent> {
        self.agents.lock().await.get(agent_id).cloned()
    }

    /// Block until a sub-agent completes, then return its result.
    pub async fn collect(&self, agent_id: &str) -> Option<String> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
        loop {
            let agent = {
                let agents = self.agents.lock().await;
                agents.get(agent_id).cloned()
            };
            match agent {
                Some(a) if a.status == AgentStatus::Completed => return a.result,
                Some(a) if a.status == AgentStatus::Failed => {
                    return a.error.map(|e| format!("子代理执行失败: {e}"));
                }
                None => return Some("子代理不存在".into()),
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Some("等待子代理结果超时".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    pub async fn list(&self) -> Vec<SubAgent> {
        self.agents.lock().await.values().cloned().collect()
    }

    /// Abort all running sub-agents. Called when parent turn ends.
    pub async fn abort_all(&self) {
        let mut agents = self.agents.lock().await;
        for agent in agents.values_mut() {
            if agent.status == AgentStatus::Running {
                agent.status = AgentStatus::Failed;
                agent.error = Some("父任务已结束，子代理被取消".into());
                agent.completed_at = Some(Utc::now().to_rfc3339());
            }
        }
    }
}

pub struct ServerAgentSessionFactory {
    repository: Arc<dyn ConversationRepository>,
    gateway: Arc<dyn ConversationModelGateway>,
    context_source: Arc<dyn ConversationContextSource>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    events: Arc<dyn ConversationEventSink>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    pool: kairos_platform_framework_database::PgPool,
}

impl ServerAgentSessionFactory {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<dyn ConversationRepository>,
        gateway: Arc<dyn ConversationModelGateway>,
        context_source: Arc<dyn ConversationContextSource>,
        knowledge_source: Arc<dyn ConversationKnowledgeSource>,
        tool_executor: Arc<dyn ConversationToolExecutor>,
        approval_gate: Arc<dyn ToolApprovalGate>,
        events: Arc<dyn ConversationEventSink>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
        pool: kairos_platform_framework_database::PgPool,
    ) -> Self {
        Self {
            repository,
            gateway,
            context_source,
            knowledge_source,
            tool_executor,
            approval_gate,
            events,
            intent_detector,
            pool,
        }
    }
}

impl AgentSessionFactory for ServerAgentSessionFactory {
    fn create(&self, max_concurrent: usize) -> Arc<dyn AgentSession> {
        Arc::new(ServerAgentSession {
            manager: AgentManager::new(max_concurrent),
            repository: self.repository.clone(),
            gateway: self.gateway.clone(),
            context_source: self.context_source.clone(),
            knowledge_source: self.knowledge_source.clone(),
            tool_executor: self.tool_executor.clone(),
            approval_gate: self.approval_gate.clone(),
            events: self.events.clone(),
            intent_detector: self.intent_detector.clone(),
            pool: self.pool.clone(),
        })
    }
}

struct ServerAgentSession {
    manager: AgentManager,
    repository: Arc<dyn ConversationRepository>,
    gateway: Arc<dyn ConversationModelGateway>,
    context_source: Arc<dyn ConversationContextSource>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    events: Arc<dyn ConversationEventSink>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    pool: kairos_platform_framework_database::PgPool,
}

#[async_trait::async_trait]
impl AgentSession for ServerAgentSession {
    async fn active_count(&self) -> usize {
        self.manager.active_count().await
    }

    async fn spawn(&self, request: AgentSpawnRequest) -> anyhow::Result<String> {
        self.manager
            .spawn(
                request.description,
                request.prompt,
                request.session_id,
                request.parent_run_id,
                request.target,
                self.repository.clone(),
                self.gateway.clone(),
                self.context_source.clone(),
                self.knowledge_source.clone(),
                self.tool_executor.clone(),
                self.approval_gate.clone(),
                self.events.clone(),
                self.intent_detector.clone(),
                self.pool.clone(),
            )
            .await
    }

    async fn status(&self, agent_id: &str) -> Option<RuntimeSubAgent> {
        self.manager.status(agent_id).await.map(runtime_sub_agent)
    }

    async fn collect(&self, agent_id: &str) -> Option<String> {
        self.manager.collect(agent_id).await
    }

    async fn list(&self) -> Vec<RuntimeSubAgent> {
        self.manager
            .list()
            .await
            .into_iter()
            .map(runtime_sub_agent)
            .collect()
    }

    async fn abort_all(&self) {
        self.manager.abort_all().await;
    }
}

fn runtime_sub_agent(agent: SubAgent) -> RuntimeSubAgent {
    RuntimeSubAgent {
        agent_id: agent.agent_id,
        description: agent.description,
        status: match agent.status {
            AgentStatus::Running => RuntimeAgentStatus::Running,
            AgentStatus::Completed => RuntimeAgentStatus::Completed,
            AgentStatus::Failed => RuntimeAgentStatus::Failed,
        },
        result: agent.result,
        error: agent.error,
        started_at: agent.started_at,
        completed_at: agent.completed_at,
    }
}

// ── Sub-agent execution ─────────────────────────────────────────────────

async fn execute_sub_agent_inline_tool(
    tool_call_id: &str,
    tool_name: &str,
    arguments: &str,
    run_id: &str,
    repository: &Arc<dyn ConversationRepository>,
    knowledge_source: &Arc<dyn ConversationKnowledgeSource>,
    pool: &kairos_platform_framework_database::PgPool,
) -> super::tool_executor::ConversationToolResult {
    let content = match tool_name {
        "knowledge_search" => {
            let parsed = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
            let query = parsed
                .get("query")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim();
            if query.is_empty() {
                Err(anyhow::anyhow!("query 不能为空"))
            } else {
                knowledge_source
                    .search_guides(query, 5)
                    .await
                    .map(|guides| serde_json::json!({"query":query,"guides":guides}))
            }
        }
        "evidence_list" | "evidence_search" | "evidence_view" => {
            repository
                .execute_evidence_tool(run_id, tool_name, arguments)
                .await
        }
        "skill_list" | "skill_load" => repository.execute_skill_tool(tool_name, arguments).await,
        "record_finding" => super::run_bridge::record_finding(pool, run_id, arguments)
            .await
            .map_err(anyhow::Error::msg),
        _ => Err(anyhow::anyhow!("未知的子代理内联工具: {tool_name}")),
    };
    super::tool_executor::ConversationToolResult {
        tool_call_id: tool_call_id.to_string(),
        tool_name: tool_name.to_string(),
        success: content.is_ok(),
        content: content
            .map(|value| value.to_string())
            .unwrap_or_else(|error| {
                serde_json::json!({"error":error.to_string(),"retryable":false}).to_string()
            }),
    }
}

/// Execute a sub-agent's autonomous tool loop independently.
/// Reuses the parent's Run for evidence isolation, supports parallel tool execution.
async fn execute_sub_agent(
    agent_id: &str,
    session_id: &str,
    prompt: &str,
    target: Option<&str>,
    description: &str,
    repository: Arc<dyn ConversationRepository>,
    gateway: Arc<dyn ConversationModelGateway>,
    context_source: Arc<dyn ConversationContextSource>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    events: Arc<dyn ConversationEventSink>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    pool: kairos_platform_framework_database::PgPool,
) -> anyhow::Result<String> {
    let objective = if let Some(t) = target {
        format!("对 {} 执行：{}", t, prompt)
    } else {
        prompt.to_string()
    };
    let request = AgentTurnRequest {
        session_id: session_id.to_string(),
        content: objective.clone(),
        message_type: ConversationMessageType::Chat,
        client_request_id: format!("sub-agent-{}", agent_id),
        attachments: vec![],
    };

    let mut context = context_source.build(&request).await?;
    context.policy_snapshot["_agent_role"] = serde_json::json!("sub");
    let supplement = super::agent_pipeline::user_context_supplement(
        &context.recent_messages,
        std::env::var("PENTA_SUB_AGENT_USER_CONTEXT_MAX_CHARS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(24_000),
    );
    if !supplement.is_empty() {
        context.recent_messages.push(AgentMessage {
            message_id: format!("sub_context:{agent_id}"),
            role: "user".into(),
            content: supplement,
            format: "text".into(),
        });
    }
    let intent = intent_detector
        .detect(&request, context.run_id.is_some())
        .await?;

    // Reuse parent's existing Run
    let run_id = repository
        .ensure_execution_run(agent_id, session_id)
        .await?
        .unwrap_or_else(|| format!("sub-run-{}", agent_id));

    let budget = AgentLoopLimits {
        max_rounds: SUB_AGENT_MAX_TOOL_ROUNDS,
        max_duration_seconds: u64::MAX,
        max_requests: SUB_AGENT_MAX_REQUESTS,
        max_tokens: u64::MAX,
        stall_limit: Some(SUB_AGENT_STALL_LIMIT),
    };
    let mut loop_engine = AgentLoopEngine::new(budget);
    let mut context = context;
    context.run_id = Some(run_id.clone());

    // Inject sub-agent instructions
    context.recent_messages.push(AgentMessage {
        message_id: format!("sub_agent_start:{}", agent_id),
        role: "user".into(),
        content: format!(
            "## 子代理任务\n\n\
             你是一个专注于单一子任务的渗透测试子代理。\n\
             任务描述：{description}\n\
             任务目标：{objective}\n\n\
             **执行规则**：\n\
             - 专注于当前子任务，不要扩展开来扫描其他目标\n\
             - 直接调用工具执行，不要输出计划等待确认\n\
             - 任务完成 → `FINAL: <结论>`（必须引用工具输出作为证据）\n\
             - 遇到问题 → 直接 REPLAN 改策略继续，不要停下\n\
             - 确实无路可走 → `NO_PATH: <原因>`\n\
             - 需要关键信息 → `ASK_USER: <问题>`\n\
             - 子代理预算有限（{} 轮），请高效执行\n\n\
             **工具选择**：优先用专项工具（nuclei/sqlmap/dalfox/dir_fuzz/ffuf/js_analyze/browser MCP），\
             curl/http_request 仅用于特殊测试。先探测后攻击，工具失败就换工具不要重试。",
            loop_engine.limits().max_rounds
        ),
        format: "text".into(),
    });

    let mut final_output: Option<String> = None;
    for round in loop_engine.next_round()..loop_engine.limits().max_rounds {
        if let Err(reason) = loop_engine.begin_round(round) {
            let state = loop_engine.checkpoint();
            final_output = Some(match reason {
                AgentLoopStop::Requests => format!(
                    "子代理请求数耗尽（{}），未完成子任务",
                    loop_engine.limits().max_requests
                ),
                AgentLoopStop::Stalled => format!(
                    "子代理连续 {} 次工具失败，判定为阻塞",
                    state.consecutive_failures
                ),
                other => format!("子代理因 {} 预算停止，未完成子任务", other.as_str()),
            });
            break;
        }

        let output = match gateway.generate(&context, &intent).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(agent_id, error = %e, "sub-agent model call failed");
                return Err(anyhow::anyhow!("子代理模型调用失败: {e}"));
            }
        };
        loop_engine.record_model_usage(output.usage.input_tokens, output.usage.output_tokens);

        if !output.tool_calls.is_empty() {
            let mut hitl_rejections = std::collections::HashMap::new();
            let mut output = output;
            for tool_call in &mut output.tool_calls {
                match super::agent_pipeline::validate_tool_call(tool_call) {
                    super::agent_pipeline::ToolCallValidation::Valid => {}
                    super::agent_pipeline::ToolCallValidation::Repairable(reason) => {
                        tracing::warn!(tool_call_id = %tool_call.id, %reason, "repaired malformed sub-agent tool arguments");
                    }
                    super::agent_pipeline::ToolCallValidation::Rejected(reason) => {
                        hitl_rejections.insert(tool_call.id.clone(), reason);
                    }
                }
            }
            for tool_call in &mut output.tool_calls {
                if hitl_rejections.contains_key(&tool_call.id) {
                    continue;
                }
                match approval_gate
                    .review(&ToolApprovalRequest {
                        run_id: run_id.clone(),
                        session_id: session_id.to_string(),
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        arguments: tool_call.arguments.clone(),
                    })
                    .await
                {
                    Ok(ToolApprovalOutcome::Approved(arguments)) => {
                        tool_call.arguments = arguments;
                    }
                    Ok(ToolApprovalOutcome::Rejected(reason)) => {
                        hitl_rejections.insert(tool_call.id.clone(), reason);
                    }
                    Err(error) => {
                        hitl_rejections
                            .insert(tool_call.id.clone(), format!("AI 围栏处理失败：{error}"));
                    }
                }
            }
            context.recent_messages.push(AgentMessage {
                message_id: format!("sub_assistant:{}:{}", agent_id, round),
                role: "assistant".into(),
                content: serde_json::json!({
                    "content": output.content,
                    "tool_calls": output.tool_calls.iter().map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {"name": tc.name, "arguments": tc.arguments}
                        })
                    }).collect::<Vec<_>>()
                })
                .to_string(),
                format: "tool_calls".into(),
            });

            // ── Parallel tool execution (fast/slow split) ──
            let mut fast_results: Vec<(
                usize,
                super::tool_executor::ConversationToolResult,
                String,
                String,
            )> = Vec::new();
            let mut slow_spawns: Vec<(
                usize,
                String,
                String,
                String,
                tokio::task::JoinHandle<(
                    String,
                    String,
                    super::tool_executor::ConversationToolResult,
                    u64,
                )>,
            )> = Vec::new();

            for (i, tc) in output.tool_calls.iter().enumerate() {
                if loop_engine.start_request().is_err() {
                    break;
                }

                if let Some(reason) = hitl_rejections.get(&tc.id) {
                    fast_results.push((
                        i,
                        super::tool_executor::ConversationToolResult {
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            content: serde_json::json!({"error":reason,"blocked":true,"blocked_by":"hitl"}).to_string(),
                            success: false,
                        },
                        tc.id.clone(),
                        tc.name.clone(),
                    ));
                    continue;
                }

                // Sub-agents cannot manage siblings or create descendants.
                if matches!(
                    tc.name.as_str(),
                    "agent_spawn" | "agent_status" | "agent_collect" | "agent_list"
                ) {
                    fast_results.push((
                        i,
                        super::tool_executor::ConversationToolResult {
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            content: serde_json::json!({"error":"子代理不允许调用智能体管理工具","blocked":true}).to_string(),
                            success: false,
                        },
                        tc.id.clone(),
                        tc.name.clone(),
                    ));
                    continue;
                }

                if matches!(
                    tc.name.as_str(),
                    "knowledge_search"
                        | "evidence_list"
                        | "evidence_search"
                        | "evidence_view"
                        | "skill_list"
                        | "skill_load"
                        | "record_finding"
                ) {
                    let result = execute_sub_agent_inline_tool(
                        &tc.id,
                        &tc.name,
                        &tc.arguments,
                        &run_id,
                        &repository,
                        &knowledge_source,
                        &pool,
                    )
                    .await;
                    fast_results.push((i, result, tc.id.clone(), tc.name.clone()));
                } else {
                    // Slow network tools — run in parallel via spawn
                    let call_id = tc.id.clone();
                    let name = tc.name.clone();
                    let args = tc.arguments.clone();
                    let scope = context.scope_snapshot.clone();
                    let sid = session_id.to_string();
                    let rid = run_id.clone();
                    let repo = repository.clone();
                    let executor = tool_executor.clone();
                    let handle = tokio::spawn(async move {
                        let start = std::time::Instant::now();
                        let result = executor
                            .execute(
                                &RuntimeToolCall {
                                    id: call_id.clone(),
                                    name: name.clone(),
                                    arguments: args.clone(),
                                },
                                &ToolExecutionContext {
                                    scope_snapshot: scope,
                                    session_id: sid,
                                    run_id: Some(rid.clone()),
                                },
                            )
                            .await;
                        let _ = repo
                            .record_tool_result(
                                &rid,
                                &call_id,
                                &name,
                                &args,
                                result.success,
                                &result.content,
                                start.elapsed().as_millis() as u64,
                            )
                            .await;
                        (call_id, name, result, start.elapsed().as_millis() as u64)
                    });
                    slow_spawns.push((
                        i,
                        tc.id.clone(),
                        tc.name.clone(),
                        tc.arguments.clone(),
                        handle,
                    ));
                }
            }

            // Await slow tools
            for (_idx, _call_id, _tool_name, _tool_args, handle) in slow_spawns {
                match handle.await {
                    Ok((cid, name, result, _elapsed)) => {
                        fast_results.push((0, result, cid, name));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "sub-agent parallel tool panicked");
                    }
                }
            }

            // Process all results
            for (_idx, result, call_id, tool_name) in &fast_results {
                let effective = has_effective_observation(tool_name, &result.content);
                loop_engine.record_tool_result(result.success, effective);

                let compact = if result.content.chars().count() > 4000 {
                    format!(
                        "{}...",
                        result.content.chars().take(4000).collect::<String>()
                    )
                } else {
                    result.content.clone()
                };
                context.recent_messages.push(AgentMessage {
                    message_id: format!("sub_tool:{}:{}", agent_id, call_id),
                    role: "tool".into(),
                    content: serde_json::json!({
                        "tool_call_id": call_id,
                        "result": compact,
                    })
                    .to_string(),
                    format: "tool_result".into(),
                });
            }
            continue;
        }

        // ── No tool calls — parse lifecycle action ──
        let content = output.content.trim();
        let directive = parse_agent_directive(content);
        let loop_checkpoint = loop_engine.checkpoint();

        match directive {
            AgentDirective::Final(text) => {
                if loop_checkpoint.tool_call_count < SUB_AGENT_MIN_TOOL_CALLS {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_gate_reject:{}", agent_id),
                        role: "user".into(),
                        content: format!(
                            "FINAL 被拒绝：尚未调用任何工具（当前 0 次，需求最少 {} 次）。\
                             请至少调用一个工具收集证据后再提交 FINAL。",
                            SUB_AGENT_MIN_TOOL_CALLS
                        ),
                        format: "text".into(),
                    });
                    continue;
                }
                if loop_checkpoint.effective_observations == 0 {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_gate_reject:{}", agent_id),
                        role: "user".into(),
                        content: "FINAL 被拒绝：所有工具调用均失败或无有效观察。请改变策略重试，或使用 NO_PATH 报告阻塞。".into(),
                        format: "text".into(),
                    });
                    continue;
                }
                final_output = Some(text);
                break;
            }
            AgentDirective::Replan(reason) => {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("sub_replan:{}:{}", agent_id, round),
                    role: "assistant".into(),
                    content: format!("REPLAN: {reason}"),
                    format: "text".into(),
                });
                context.recent_messages.push(AgentMessage {
                    message_id: format!("sub_replan_prompt:{}", agent_id),
                    role: "user".into(),
                    content: "请根据新计划直接选择并调用工具，不要只输出计划。".into(),
                    format: "text".into(),
                });
                continue;
            }
            AgentDirective::NoPath(reason) => {
                if loop_checkpoint.tool_call_count == 0 {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_no_path_reject:{}", agent_id),
                        role: "user".into(),
                        content: "NO_PATH 被拒绝：尚未调用任何工具，不能判定无可行路径。请先尝试调用工具。".into(),
                        format: "text".into(),
                    });
                    continue;
                }
                final_output = Some(format!("NO_PATH: {reason}"));
                break;
            }
            AgentDirective::AskUser(question) => {
                final_output = Some(format!("ASK_USER: {question}"));
                break;
            }
            AgentDirective::Candidate(text) => {
                if loop_checkpoint.tool_call_count >= SUB_AGENT_MIN_TOOL_CALLS
                    && loop_checkpoint.effective_observations > 0
                {
                    final_output = Some(text);
                } else {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_no_tools:{}", agent_id),
                        role: "user".into(),
                        content: "请至少调用一个工具并对目标进行实际验证后再给出结论。".into(),
                        format: "text".into(),
                    });
                    continue;
                }
                break;
            }
        }
    }

    let loop_checkpoint = loop_engine.checkpoint();
    let result = final_output.unwrap_or_else(|| {
        format!(
            "子代理 {}: {} 轮 {} 次请求后未生成结论（工具调用 {} 次，有效观察 {} 次）",
            description,
            loop_engine.limits().max_rounds,
            loop_checkpoint.request_count,
            loop_checkpoint.tool_call_count,
            loop_checkpoint.effective_observations
        )
    });

    Ok(result)
}

fn has_effective_observation(tool_name: &str, content: &str) -> bool {
    if content.contains(r#""blocked":true"#) || content.contains(r#""error":"#) {
        return false;
    }
    match tool_name {
        "knowledge_search" | "evidence_list" | "evidence_search" | "evidence_view"
        | "skill_list" | "skill_load" | "record_finding" | "agent_status" | "agent_collect"
        | "agent_list" => false,
        _ => !content.trim().is_empty(),
    }
}
