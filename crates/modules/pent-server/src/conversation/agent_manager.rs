//! Agent manager for concurrent sub-agent execution.
//!
//! Supports model-driven task decomposition: the model can call `agent_spawn`
//! to create sub-agents that run independent tool loops concurrently, then
//! `agent_collect` to gather results.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::Utc;
use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, ConversationContext, ConversationIntent,
    ConversationMessageType,
};

use super::{
    ConversationContextSource, ConversationEventSink, ConversationIntentDetector,
    ConversationKnowledgeSource, ConversationModelGateway, ConversationRepository,
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
                    return a.error.map(|e| format!("子代理执行失败: {e}"))
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

// ── Sub-agent execution ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum SubAgentDirective {
    Final(String),
    Replan(String),
    NoPath(String),
    AskUser(String),
    Candidate(String),
}

fn parse_sub_agent_directive(content: &str) -> SubAgentDirective {
    let content = content.trim();

    // JSON action protocol
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(action) = parsed.get("action").and_then(serde_json::Value::as_str) {
            let body = parsed
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let fallback = if body.is_empty() {
                content.to_string()
            } else {
                body
            };
            return match action {
                "final" | "FINAL" => SubAgentDirective::Final(fallback),
                "replan" | "REPLAN" => SubAgentDirective::Replan(fallback),
                "no_path" | "NO_PATH" => SubAgentDirective::NoPath(fallback),
                "ask_user" | "ASK_USER" => SubAgentDirective::AskUser(fallback),
                _ => SubAgentDirective::Candidate(content.to_string()),
            };
        }
    }

    // Text prefix protocol
    for marker in ["FINAL:", "Final:", "final:", "最终结果：", "完成："] {
        if let Some(v) = content.strip_prefix(marker) {
            return SubAgentDirective::Final(v.trim().to_string());
        }
    }
    for marker in ["REPLAN:", "Replan:", "replan:", "重新规划："] {
        if let Some(v) = content.strip_prefix(marker) {
            return SubAgentDirective::Replan(v.trim().to_string());
        }
    }
    for marker in [
        "NO_PATH:",
        "no_path:",
        "无法继续：",
        "没有可继续验证的路径：",
    ] {
        if let Some(v) = content.strip_prefix(marker) {
            return SubAgentDirective::NoPath(v.trim().to_string());
        }
    }
    for marker in ["ASK_USER:", "ask_user:", "需要用户："] {
        if let Some(v) = content.strip_prefix(marker) {
            return SubAgentDirective::AskUser(v.trim().to_string());
        }
    }

    SubAgentDirective::Candidate(content.to_string())
}

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
            let parsed = serde_json::from_str::<serde_json::Value>(arguments)
                .unwrap_or_default();
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
        "evidence_list" | "evidence_search" | "evidence_view" => repository
            .execute_evidence_tool(run_id, tool_name, arguments)
            .await,
        "skill_list" | "skill_load" => repository
            .execute_skill_tool(tool_name, arguments)
            .await,
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

    let budget = SubAgentBudget::default();
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
            budget.max_rounds
        ),
        format: "text".into(),
    });

    let mut final_output: Option<String> = None;
    let mut request_count = 0usize;
    let mut tool_call_count = 0usize;
    let mut consecutive_failures = 0usize;
    let mut effective_observations = 0usize;

    for round in 0..budget.max_rounds {
        if request_count >= budget.max_requests {
            final_output = Some(format!(
                "子代理请求数耗尽（{}），未完成子任务",
                budget.max_requests
            ));
            break;
        }
        if consecutive_failures >= SUB_AGENT_STALL_LIMIT {
            final_output = Some(format!(
                "子代理连续 {} 次工具失败，判定为阻塞",
                consecutive_failures
            ));
            break;
        }

        let output = match gateway.generate(&context, &intent).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(agent_id, error = %e, "sub-agent model call failed");
                return Err(anyhow::anyhow!("子代理模型调用失败: {e}"));
            }
        };

        if !output.tool_calls.is_empty() {
            let gate = super::hitl::HitlGate::new(pool.clone());
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
                match gate
                    .review(
                        &run_id,
                        session_id,
                        &tool_call.id,
                        &tool_call.name,
                        &tool_call.arguments,
                    )
                    .await
                {
                    Ok(super::hitl::HitlOutcome::Approved(arguments)) => {
                        tool_call.arguments = arguments;
                    }
                    Ok(super::hitl::HitlOutcome::Rejected(reason)) => {
                        hitl_rejections.insert(tool_call.id.clone(), reason);
                    }
                    Err(error) => {
                        hitl_rejections.insert(
                            tool_call.id.clone(),
                            format!("AI 围栏处理失败：{error}"),
                        );
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
                if request_count >= budget.max_requests {
                    break;
                }
                request_count += 1;

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
                    let handle = tokio::spawn(async move {
                        let start = std::time::Instant::now();
                        let result =
                            super::tool_executor::execute_tool(&name, &args, &scope, &sid, Some(rid.as_str())).await;
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
                        (
                            call_id,
                            name,
                            result,
                            start.elapsed().as_millis() as u64,
                        )
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
                tool_call_count += 1;
                if result.success
                    && has_effective_observation(tool_name, &result.content)
                {
                    effective_observations += 1;
                    consecutive_failures = 0;
                } else {
                    consecutive_failures += 1;
                }

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
        let directive = parse_sub_agent_directive(content);

        match directive {
            SubAgentDirective::Final(text) => {
                if tool_call_count < SUB_AGENT_MIN_TOOL_CALLS {
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
                if effective_observations == 0 {
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
            SubAgentDirective::Replan(reason) => {
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
            SubAgentDirective::NoPath(reason) => {
                if tool_call_count == 0 {
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
            SubAgentDirective::AskUser(question) => {
                final_output = Some(format!("ASK_USER: {question}"));
                break;
            }
            SubAgentDirective::Candidate(text) => {
                if tool_call_count >= SUB_AGENT_MIN_TOOL_CALLS
                    && effective_observations > 0
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

    let result = final_output.unwrap_or_else(|| {
        format!(
            "子代理 {}: {} 轮 {} 次请求后未生成结论（工具调用 {} 次，有效观察 {} 次）",
            description, budget.max_rounds, request_count, tool_call_count, effective_observations
        )
    });

    Ok(result)
}

fn has_effective_observation(tool_name: &str, content: &str) -> bool {
    if content.contains(r#""blocked":true"#) || content.contains(r#""error":"#) {
        return false;
    }
    match tool_name {
        "knowledge_search"
        | "evidence_list"
        | "evidence_search"
        | "evidence_view"
        | "skill_list"
        | "skill_load"
        | "record_finding"
        | "agent_status"
        | "agent_collect"
        | "agent_list" => false,
        _ => !content.trim().is_empty(),
    }
}

struct SubAgentBudget {
    max_rounds: usize,
    max_requests: usize,
}

impl Default for SubAgentBudget {
    fn default() -> Self {
        Self {
            max_rounds: SUB_AGENT_MAX_TOOL_ROUNDS,
            max_requests: SUB_AGENT_MAX_REQUESTS,
        }
    }
}
