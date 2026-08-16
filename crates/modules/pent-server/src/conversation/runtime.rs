use std::sync::Arc;

use chrono::Utc;
use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
    ConversationMessageType, ConversationTurnStatus,
};

use super::rmos::{
    AttackGraph, KnowledgeCache, PentestMemory,
    injector::{build_injected_context, prepare_injection},
};
use super::{
    AgentSession, AgentSessionFactory, AgentSpawnRequest, ConversationContextSource,
    ConversationEventSink, ConversationInstructionSink, ConversationIntentDetector,
    ConversationKnowledgeSource, ConversationModelGateway, ConversationRepository,
    ConversationToolExecutor, ModelTurnOutput, RuntimeAgentStatus, RuntimeToolCall,
    ToolExecutionContext,
};
use kairos_platform_pent_runtime::agent_loop::{
    AgentDirective, AgentLoopCheckpoint, AgentLoopEngine, AgentLoopLimits, parse_agent_directive,
};

/// Maximum tool-calling rounds before forcing a stop.
/// PentAGI CLI uses 3 hours default runtime; the conversation agent
/// limits rounds instead. 50 rounds ≈ 1-2 hours of typical execution.
const MAX_TOOL_ROUNDS: usize = 50;
/// Absolute maximum runtime for a conversation turn (2 hours).
const MAX_TURN_RUNTIME_SECS: u64 = 2 * 60 * 60;
/// Tool observations sent back to the model are compact. The complete response
/// has already been persisted as Evidence before this representation is used.
const MAX_MODEL_TOOL_OBSERVATION_CHARS: usize = 8_000;
const MAX_RUN_SUMMARY_CHARS: usize = 12_000;
const DEFAULT_MAX_CONSECUTIVE_MODEL_ERRORS: usize = 3;

/// Autonomous loop injection message sent to the model at the start of every
/// tool loop. This reminds the model it is operating in autonomous mode and
/// must directly call tools or emit lifecycle actions.
const AUTONOMOUS_LOOP_PROMPT: &str = "\

## 自主工具循环已启动\n\n\

你现在处于自主执行模式。根据目标、当前证据和可用工具直接选择下一步操作：\n\

- 需要信息 → 直接发起 Tool Calling\n\

- 任务完成 → 输出 `FINAL: <结论>`（事实性结论必须引用 Evidence ID）\n\

- 所有路径均失败 → 输出 `NO_PATH: <原因>` 说明已尝试内容和阻塞点\n\

- 无可行路径 → 输出 `NO_PATH: <原因>`\n\

- 策略失败需调整 → 输出 `REPLAN: <新策略>` 并在下一轮直接执行\n\n\

## ⚠ 工具选择规则（严格遵守）\n\

不要用 curl/http_request 手工做所有事！每个任务有专用工具：\n\

- 端口扫描 → nmap（不要 curl 逐个端口试）\n\

- 漏洞扫描 → nuclei / web_vuln_scan（不要 curl 手工测）\n\

- XSS 检测 → dalfox / web_xss_scan（不要 curl 试 payload）\n\

- SQL 注入 → sqlmap / sqlmap_scan（不要 curl 手工注入）\n\

- 目录爆破 → ffuf / gobuster / dir_fuzz（不要手工猜路径）\n\

- 技术栈识别 → whatweb（不要手工看响应头）\n\

- 子域名枚举 → subfinder / amass（不要手工 DNS）\n\

- CSP 头分析 → csp_analyze（不要手工读 header）\n\

- JS 安全审计 → js_security_audit（不要手工看代码）\n\

- Wiki 知识查询 → knowledge_search（不要凭记忆猜）\n\

- 浏览器操作 → mcp_navigate_page / mcp_take_screenshot（不要 curl）\n\

\n\

每次执行前先问自己：有没有专用工具能做这件事？有就用专用工具，不要用 curl/http_request 替代。\n\



不要输出等待确认的计划、不要枚举工具列表、不要在工具调用前请求许可。";

#[derive(Debug, Clone, Copy)]
struct TurnBudget {
    max_tool_rounds: usize,
    max_duration_seconds: u64,
    max_requests: usize,
    max_tokens: u64,
}

impl TurnBudget {
    fn from_snapshot(value: &serde_json::Value) -> Self {
        if std::env::var("PENTA_UNLIMITED_BUDGETS")
            .map(|value| value != "false")
            .unwrap_or(false)
        {
            return Self {
                max_tool_rounds: usize::MAX,
                max_duration_seconds: u64::MAX,
                max_requests: usize::MAX,
                max_tokens: u64::MAX,
            };
        }
        let mut budget = Self {
            max_tool_rounds: budget_u64(value, &["max_tool_rounds", "maxToolRounds"], 50)
                .clamp(1, MAX_TOOL_ROUNDS as u64) as usize,
            max_duration_seconds: budget_u64(
                value,
                &[
                    "max_duration_seconds",
                    "maxDurationSeconds",
                    "max_time_seconds",
                ],
                MAX_TURN_RUNTIME_SECS,
            )
            .clamp(1, MAX_TURN_RUNTIME_SECS),
            max_requests: budget_u64(value, &["max_requests", "maxRequests"], 200).clamp(1, 10_000)
                as usize,
            max_tokens: budget_u64(value, &["max_tokens", "maxTokens"], 200_000)
                .clamp(1, 2_000_000),
        };
        if std::env::var("PENTA_ENV").as_deref() == Ok("development") {
            budget.max_tokens = std::env::var("PENTA_DEV_MAX_TOKENS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2_000_000)
                .clamp(1, 10_000_000);
        }
        budget
    }

    fn loop_limits(self) -> AgentLoopLimits {
        AgentLoopLimits {
            max_rounds: self.max_tool_rounds,
            max_duration_seconds: self.max_duration_seconds,
            max_requests: self.max_requests,
            max_tokens: self.max_tokens,
            stall_limit: None,
        }
    }
}

fn budget_u64(value: &serde_json::Value, keys: &[&str], default: u64) -> u64 {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
        .unwrap_or(default)
}

pub struct ConversationRuntime {
    repository: Arc<dyn ConversationRepository>,
    context_source: Arc<dyn ConversationContextSource>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    model_gateway: Arc<dyn ConversationModelGateway>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    instruction_sink: Arc<dyn ConversationInstructionSink>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    agent_sessions: Arc<dyn AgentSessionFactory>,
    events: Arc<dyn ConversationEventSink>,
    nats: Option<crate::transport::mq::NatsHandle>,
    pool: kairos_platform_framework_database::PgPool,
}

impl ConversationRuntime {
    pub fn new(
        repository: Arc<dyn ConversationRepository>,
        context_source: Arc<dyn ConversationContextSource>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
        model_gateway: Arc<dyn ConversationModelGateway>,
        knowledge_source: Arc<dyn ConversationKnowledgeSource>,
        instruction_sink: Arc<dyn ConversationInstructionSink>,
        tool_executor: Arc<dyn ConversationToolExecutor>,
        agent_sessions: Arc<dyn AgentSessionFactory>,
        events: Arc<dyn ConversationEventSink>,
        pool: kairos_platform_framework_database::PgPool,
    ) -> Self {
        Self {
            repository,
            context_source,
            intent_detector,
            model_gateway,
            knowledge_source,
            instruction_sink,
            tool_executor,
            agent_sessions,
            events,
            nats: None,
            pool,
        }
    }

    pub fn with_nats(mut self, nats: crate::transport::mq::NatsHandle) -> Self {
        self.nats = Some(nats);
        self
    }

    /// Create the turn and user message in the database, return (turn_id, run_id).
    /// The turn status is "thinking" — the caller is expected to call `execute_turn`
    /// in a background task.
    pub async fn begin_turn_only(
        &self,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<(String, Option<String>)> {
        let intent = ConversationIntent {
            kind: ConversationMessageType::Chat,
            confidence: 0.5,
            requires_run_instruction: false,
            requires_confirmation: false,
        };
        self.repository.begin_turn(request, &intent).await
    }

    /// Execute the tool loop for an existing turn. Updates turn status on completion.
    pub async fn execute_turn(
        &self,
        turn_id: String,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<()> {
        if let Err(error) = self.execute_turn_inner(&turn_id, request).await {
            if let Err(mark_error) = self
                .repository
                .fail_turn(&turn_id, "TURN_EXECUTION_FAILED", &error.to_string(), true)
                .await
            {
                tracing::error!(
                    turn_id,
                    error = %mark_error,
                    "failed to persist conversation turn failure"
                );
            }
            return Err(error);
        }
        Ok(())
    }

    async fn execute_turn_inner(
        &self,
        turn_id: &str,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<()> {
        self.repository
            .ensure_execution_run(turn_id, &request.session_id)
            .await?;
        let context = self.context_source.build(request).await?;
        let intent = self
            .intent_detector
            .detect(request, context.run_id.is_some())
            .await?;
        let run_id = context.run_id.clone();

        self.emit_event(
            "conversation.turn.started",
            serde_json::json!({"turn_id": turn_id, "session_id": request.session_id}),
        )
        .await;
        // Write run event for the Run detail page
        if let Some(ref run_id) = run_id {
            self.repository
                .record_run_event(
                    run_id,
                    "run.turn_started",
                    &serde_json::json!({"turn_id": turn_id}),
                )
                .await?;
        }

        let output = self
            .run_tool_loop(turn_id, &request.session_id, context, &intent)
            .await?;

        let result = AgentTurnResult {
            turn_id: turn_id.to_owned(),
            session_id: request.session_id.clone(),
            run_id,
            status: ConversationTurnStatus::Completed,
            message: AgentMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".into(),
                content: output.clone(),
                format: "markdown".into(),
            },
            intent,
            plan_update: None,
            observations: vec![],
            findings: vec![],
            citations: vec![],
            usage: Default::default(),
            error: None,
            completed_at: Some(Utc::now()),
        };
        if let Err(errors) = kairos_platform_pent_core::conversation::validate_turn_result(&result)
        {
            let error = anyhow::anyhow!("模型输出校验失败: {}", errors.join(","));
            self.repository
                .fail_turn(&turn_id, "INVALID_MODEL_OUTPUT", &error.to_string(), true)
                .await?;
            return Err(error);
        }
        self.repository.complete_turn(&result).await?;
        self.events
            .emit(
                "conversation.message.completed",
                serde_json::to_value(&result)?,
            )
            .await?;

        // 交互式会话路径也要在 Run 完成后提取并落库已确认漏洞。
        // 自主 Worker 路径由 `execute_session_run` 负责提取，这里只兜底 UI 路径。
        if let Some(run_id) = result.run_id.as_deref() {
            match super::run_bridge::extract_and_persist_findings(
                &self.pool,
                run_id,
                &request.session_id,
            )
            .await
            {
                Ok(count) if count > 0 => {
                    if let Err(error) = self
                        .repository
                        .record_run_event(
                            run_id,
                            "run.findings.persisted",
                            &serde_json::json!({"count": count}),
                        )
                        .await
                    {
                        tracing::warn!(run_id, error = %error, "failed to record findings event");
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(run_id, error = %error, "vulnerability extraction failed");
                }
            }
        }

        Ok(())
    }

    pub async fn handle(&self, request: &AgentTurnRequest) -> anyhow::Result<AgentTurnResult> {
        let context = self.context_source.build(request).await?;
        let intent = self
            .intent_detector
            .detect(request, context.run_id.is_some())
            .await?;
        let (turn_id, run_id) = self.repository.begin_turn(request, &intent).await?;
        self.events
            .emit(
                "conversation.turn.created",
                serde_json::json!({"turn_id": turn_id, "session_id": request.session_id}),
            )
            .await?;

        if intent.requires_run_instruction {
            let active_run = match run_id.as_deref() {
                Some(active_run) => active_run,
                None => {
                    let error = anyhow::anyhow!("运行指令缺少 active run");
                    self.repository
                        .fail_turn(&turn_id, "RUN_NOT_ACTIVE", &error.to_string(), false)
                        .await?;
                    return Err(error);
                }
            };
            if let Err(error) = self
                .instruction_sink
                .submit(active_run, request, &intent)
                .await
            {
                self.repository
                    .fail_turn(&turn_id, "INSTRUCTION_REJECTED", &error.to_string(), false)
                    .await?;
                return Err(error);
            }
        }

        let final_content = self
            .run_tool_loop(&turn_id, &request.session_id, context, &intent)
            .await?;

        let output = ModelTurnOutput {
            content: final_content.clone(),
            reasoning_summary: None,
            plan_update: None,
            citations: vec![],
            usage: Default::default(),
            tool_calls: vec![],
        };
        let result = AgentTurnResult {
            turn_id,
            session_id: request.session_id.clone(),
            run_id,
            status: ConversationTurnStatus::Completed,
            message: AgentMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".into(),
                content: final_content,
                format: "markdown".into(),
            },
            intent,
            plan_update: output.plan_update,
            observations: vec![],
            findings: vec![],
            citations: output.citations,
            usage: Default::default(),
            error: None,
            completed_at: Some(Utc::now()),
        };
        if let Err(errors) = kairos_platform_pent_core::conversation::validate_turn_result(&result)
        {
            let error = anyhow::anyhow!("模型输出校验失败: {}", errors.join(","));
            self.repository
                .fail_turn(
                    &result.turn_id,
                    "INVALID_MODEL_OUTPUT",
                    &error.to_string(),
                    true,
                )
                .await?;
            return Err(error);
        }
        self.repository.complete_turn(&result).await?;
        self.events
            .emit(
                "conversation.message.completed",
                serde_json::to_value(&result)?,
            )
            .await?;
        Ok(result)
    }

    /// Run the tool-calling loop and return the final content.
    /// Intermediate results are written to the database as conversation messages
    /// so the SSE stream can relay them to the frontend.
    pub(crate) async fn run_tool_loop(
        &self,
        turn_id: &str,
        session_id: &str,
        mut context: ConversationContext,
        intent: &ConversationIntent,
    ) -> anyhow::Result<String> {
        let mut final_output: Option<super::ports::ModelTurnOutput> = None;
        let mut budget = TurnBudget::from_snapshot(&context.budget_snapshot);
        if let Some(max_iterations) = context
            .policy_snapshot
            .pointer("/multi_agent/max_iterations")
            .and_then(serde_json::Value::as_u64)
        {
            budget.max_tool_rounds = (max_iterations as usize).clamp(1, 10_000);
        }
        let checkpoint = context
            .policy_snapshot
            .get("_checkpoint")
            .cloned()
            .unwrap_or_default();
        let start_round = checkpoint
            .get("round")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        // Per-turn budget counts actual model API usage below. Historical
        // messages are windowed by the model gateway and are already included
        // in each provider-reported input_tokens value.
        let mut loop_engine = AgentLoopEngine::from_checkpoint(
            budget.loop_limits(),
            AgentLoopCheckpoint {
                next_round: start_round,
                request_count: checkpoint
                    .get("request_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize,
                token_count: checkpoint
                    .get("token_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                effective_observations: checkpoint
                    .get("successful_observations")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize,
                ..Default::default()
            },
        );
        let mut stopped_by_budget: Option<&'static str> = None;
        let mut finished_normally = false;
        let mut evidence_ids = Vec::new();
        let mut run_summary = context.run_summary.clone();
        let max_consecutive_model_errors = std::env::var("PENTA_MAX_CONSECUTIVE_MODEL_ERRORS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_CONSECUTIVE_MODEL_ERRORS)
            .clamp(1, 5);
        let mut consecutive_model_errors = 0;
        let mut stall_terminal_prompted = false;

        let user_input = context
            .recent_messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let scan_goal = format!("{user_input}\n{}", context.objective_snapshot);
        let mut scan_progress = super::scan_progress::ScanProgress::new(&scan_goal);
        let agent_session = self.agent_sessions.create(
            std::env::var("PENTA_MAX_AGENTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
        );
        let orchestration =
            super::agent_pipeline::OrchestrationMode::from_policy(&context.policy_snapshot);
        let orchestration_instruction =
            if super::agent_pipeline::multi_agent_enabled(&context.policy_snapshot) {
                orchestration.instruction()
            } else {
                "多智能体已关闭：当前代理直接完成任务，不要尝试调用 agent_spawn/agent_collect。"
            };
        context.recent_messages.push(AgentMessage {
            message_id: "runtime_orchestration_mode".into(),
            role: "user".into(),
            content: orchestration_instruction.into(),
            format: "text".into(),
        });
        if let Some(run_id) = context.run_id.as_deref() {
            let history = self.repository.load_run_tool_history(run_id).await?;
            evidence_ids = history.evidence_ids;
            for historical in history.tool_calls {
                let effective_observation =
                    historical.success && !is_reference_tool(&historical.tool_name);
                scan_progress.restore_tool_result(
                    &historical.tool_name,
                    historical.success,
                    effective_observation,
                    historical.semantic_fingerprint.as_deref(),
                );
            }
        }

        // Inject the autonomous loop prompt at the start of tool execution.
        // This reminds the model it is in autonomous mode and must directly
        // call tools or emit lifecycle actions — no waiting for approval.
        context.recent_messages.push(AgentMessage {
            message_id: "runtime_autonomous_loop_start".into(),
            role: "user".into(),
            content: AUTONOMOUS_LOOP_PROMPT.to_string(),
            format: "text".into(),
        });

        // ── RMOS: Memory + Knowledge + Attack Graph ──────────────────
        let mut pentest_memory = PentestMemory::new(&scan_goal);
        let mut knowledge_cache = KnowledgeCache::new();
        let mut attack_graph = AttackGraph::new();

        // ── Planning phase (before tool execution) ──────────────────
        // When the input is complex, inject a planning prompt that
        // guides the model to think through the approach first.
        if user_input.chars().count() >= 60
            || user_input.contains("http://")
            || user_input.contains("https://")
            || user_input.chars().any(|c| c > '\u{2e80}')
        {
            let planning_instruction = format!(
                "\n\n## 攻击计划\n\
                 目标：{user_input}\n\n\
                 在调用工具之前，先用一段话描述你的攻击计划：\n\
                 1. 信息收集阶段用什么工具（nmap, subfinder, whatweb 等）\n\
                 2. 漏洞扫描阶段用什么工具（nuclei, sqlmap, dalfox 等）\n\
                 3. 验证阶段怎么确认漏洞\n\n\
                 然后立即开始执行第一步，直接调用工具。"
            );
            context.recent_messages.push(AgentMessage {
                message_id: "runtime_planning_prompt".into(),
                role: "user".into(),
                content: planning_instruction,
                format: "text".into(),
            });
        }

        for round in loop_engine.next_round()..loop_engine.limits().max_rounds {
            if let Err(reason) = loop_engine.begin_round(round) {
                stopped_by_budget = Some(reason.as_str());
                break;
            }

            // ── RMOS: inject memory + knowledge + attack chain ──
            let injection = prepare_injection(
                &self.pool,
                &pentest_memory,
                &mut knowledge_cache,
                &attack_graph,
            )
            .await;
            let rmos_context = build_injected_context(&injection);
            if !rmos_context.is_empty() {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("rmos_injection_{}", round),
                    role: "system".into(),
                    content: rmos_context,
                    format: "text".into(),
                });
            }

            let mut output = match self.model_gateway.generate(&context, intent).await {
                Ok(output) => output,
                Err(error) => {
                    consecutive_model_errors += 1;
                    let transient = super::agent_pipeline::is_transient_model_error(&error);
                    if transient && consecutive_model_errors < max_consecutive_model_errors {
                        tracing::warn!(
                            error = %error,
                            attempt = consecutive_model_errors,
                            max_attempts = max_consecutive_model_errors,
                            "model call failed inside agent loop; retrying"
                        );
                        context.recent_messages.push(AgentMessage {
                            message_id: format!("runtime_model_error:round_{round}"),
                            role: "user".into(),
                            content: format!(
                                "运行时报告：上一次模型调用发生临时错误（{}）。\
                                 请保持当前目标和已有 Evidence，继续执行；不要重复已经失败的策略。",
                                compact_runtime_error(&error.to_string())
                            ),
                            format: "text".into(),
                        });
                        let backoff = (500_u64
                            .saturating_mul(1_u64 << consecutive_model_errors.saturating_sub(1)))
                        .min(30_000);
                        tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                        continue;
                    }
                    self.repository
                        .fail_turn(turn_id, "MODEL_UNAVAILABLE", &error.to_string(), transient)
                        .await?;
                    return Err(anyhow::anyhow!(
                        "模型调用连续失败 {consecutive_model_errors} 次: {error}"
                    ));
                }
            };
            loop_engine.record_model_usage(output.usage.input_tokens, output.usage.output_tokens);

            // An empty completion is recoverable: keep the agent loop alive and
            // explicitly ask the next model attempt to resume from its context.
            if output.tool_calls.is_empty() && output.content.trim().is_empty() {
                consecutive_model_errors += 1;
                if consecutive_model_errors < max_consecutive_model_errors {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_empty_model_response:round_{round}"),
                        role: "user".into(),
                        content: "运行时报告：上一条模型响应为空，没有正文或有效工具调用。\
                                  请保留当前目标和已有 Evidence，选择可用工具继续执行；\
                                  若确实无法继续，请明确返回 ASK_USER 或 NO_PATH。"
                            .into(),
                        format: "text".into(),
                    });
                    tokio::time::sleep(std::time::Duration::from_millis(
                        250 * consecutive_model_errors as u64,
                    ))
                    .await;
                    continue;
                }
                self.repository
                    .fail_turn(
                        turn_id,
                        "MODEL_EMPTY_RESPONSE",
                        &format!("模型连续 {consecutive_model_errors} 次返回了空内容"),
                        true,
                    )
                    .await?;
                return Err(anyhow::anyhow!(
                    "模型连续 {consecutive_model_errors} 次返回了空内容"
                ));
            }
            consecutive_model_errors = 0;

            if !output.tool_calls.is_empty() {
                // Every tool path, including inline tools and agent_spawn, crosses
                // the same DesRed-compatible HITL fence before execution.
                let mut hitl_rejections = std::collections::HashMap::new();
                let plan_required = orchestration
                    == super::agent_pipeline::OrchestrationMode::PlanExecute
                    && if let Some(run_id) = context.run_id.as_deref() {
                        !super::plan_board::exists(&self.pool, run_id)
                            .await
                            .unwrap_or(false)
                    } else {
                        false
                    };
                for tool_call in &mut output.tool_calls {
                    if orchestration == super::agent_pipeline::OrchestrationMode::Supervisor
                        && !matches!(
                            tool_call.name.as_str(),
                            "agent_spawn" | "agent_status" | "agent_collect" | "agent_list"
                        )
                    {
                        hitl_rejections.insert(
                            tool_call.id.clone(),
                            "supervisor 模式的主代理只能路由、监督和汇总子代理；请使用 agent_spawn/agent_collect"
                                .into(),
                        );
                        continue;
                    }
                    if plan_required && tool_call.name != "plan_create" {
                        hitl_rejections.insert(
                            tool_call.id.clone(),
                            "plan_execute 模式必须先调用 plan_create 创建持久化计划".into(),
                        );
                        continue;
                    }
                    match super::agent_pipeline::validate_tool_call(tool_call) {
                        super::agent_pipeline::ToolCallValidation::Valid => {}
                        super::agent_pipeline::ToolCallValidation::Repairable(reason) => {
                            tracing::warn!(tool_call_id = %tool_call.id, %reason, "repaired malformed model tool arguments");
                        }
                        super::agent_pipeline::ToolCallValidation::Rejected(reason) => {
                            hitl_rejections.insert(tool_call.id.clone(), reason);
                        }
                    }
                }
                if let Some(run_id) = context.run_id.as_deref() {
                    let gate = super::hitl::HitlGate::new(self.pool.clone());
                    for tool_call in &mut output.tool_calls {
                        if hitl_rejections.contains_key(&tool_call.id) {
                            continue;
                        }
                        match gate
                            .review(
                                run_id,
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
                                // Fail closed when policy loading or persistence fails.
                                hitl_rejections.insert(
                                    tool_call.id.clone(),
                                    format!("AI 围栏处理失败：{error}"),
                                );
                            }
                        }
                    }
                }
                // Write assistant tool_calls message to DB so SSE can stream it
                let tool_calls_json: Vec<serde_json::Value> = output
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments
                            }
                        })
                    })
                    .collect();

                // Persist the model's optional reasoning/plan together with
                // tool_calls so it remains part of the native tool transcript.
                let assistant_tool_message = serde_json::json!({
                    "content": if output.content.trim().is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(output.content.clone())
                    },
                    "tool_calls": tool_calls_json
                });
                self.repository
                    .write_intermediate_message(
                        turn_id,
                        session_id,
                        "assistant",
                        &assistant_tool_message.to_string(),
                        "tool_calls",
                    )
                    .await?;

                context.recent_messages.push(AgentMessage {
                    message_id: format!("assistant_tool:round_{}", round),
                    role: "assistant".into(),
                    content: assistant_tool_message.to_string(),
                    format: "tool_calls".into(),
                });

                // ── Phase pushes: guide the model through a systematic workflow ──
                if round == 2 {
                    context.recent_messages.push(AgentMessage {
                        message_id: "runtime_push_js".into(),
                        role: "user".into(),
                        content: "第一步存活确认已完成。请立即对网站进行 JavaScript 分析：\
                                  用 js_collect 收集所有 JS 文件，然后用 js_analyze 分析提取 \
                                  API 端点、密钥、内部路径。这比逐一手工测试高效得多。"
                            .into(),
                        format: "text".into(),
                    });
                }
                if round == 3 {
                    context.recent_messages.push(AgentMessage {
                        message_id: "runtime_push_vuln_scan".into(),
                        role: "user".into(),
                        content:
                            "信息收集阶段基本完成。请立即切换策略：用专项自动扫描工具（漏洞扫描、\
                                  XSS 检测、SQL 注入检测、目录爆破）批量发现漏洞，\
                                  用浏览器工具验证交互式漏洞，不要再逐一手工测试。"
                                .into(),
                        format: "text".into(),
                    });
                }

                // ── Force summary when approaching turn limit ──────────
                if round == 15 {
                    context.recent_messages.push(AgentMessage {
                        message_id: "runtime_force_summary".into(), role: "user".into(),
                        content: "已执行大量工具调用。请立即停止调用工具，用纯文本总结所有发现和漏洞。不要调用任何工具。".into(),
                        format: "text".into(),
                    });
                }
                if round > 15 {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_stop_{round}"),
                        role: "user".into(),
                        content: "停止调用工具，输出最终报告。".into(),
                        format: "text".into(),
                    });
                }

                let mut loop_diagnostic = None;

                // ── Phase 1: group tools by capability ──
                // Read/info tools → concurrent. Destructive/write tools → sequential.
                // Inline tools (knowledge, evidence, traffic, skill, agent) → immediate.
                let mut fast_results: Vec<(
                    usize,
                    super::tool_executor::ConversationToolResult,
                    String,
                    String,
                    std::time::Duration,
                )> = Vec::new();
                let mut read_spawns: Vec<(
                    usize,
                    String,
                    String,
                    String,
                    tokio::task::JoinHandle<(
                        String,
                        String,
                        String,
                        super::tool_executor::ConversationToolResult,
                        u64,
                    )>,
                )> = Vec::new();
                let mut destructive_spawns: Vec<(
                    usize,
                    String,
                    String,
                    String,
                    tokio::task::JoinHandle<(
                        String,
                        String,
                        String,
                        super::tool_executor::ConversationToolResult,
                        u64,
                    )>,
                )> = Vec::new();

                for (i, tc) in output.tool_calls.iter().enumerate() {
                    if let Err(reason) = loop_engine.start_request() {
                        stopped_by_budget = Some(reason.as_str());
                        break;
                    }
                    let tool_started = std::time::Instant::now();
                    if let Some(reason) = hitl_rejections.get(&tc.id) {
                        fast_results.push((
                            i,
                            super::tool_executor::ConversationToolResult {
                                tool_call_id: tc.id.clone(),
                                tool_name: tc.name.clone(),
                                content: serde_json::json!({
                                    "error": reason,
                                    "blocked": true,
                                    "blocked_by": "hitl"
                                })
                                .to_string(),
                                success: false,
                            },
                            tc.id.clone(),
                            tc.name.clone(),
                            tool_started.elapsed(),
                        ));
                        continue;
                    }
                    if is_inline_tool(&tc.name) {
                        let result = self
                            .execute_fast_tool(tc, session_id, &context, &agent_session)
                            .await;
                        fast_results.push((
                            i,
                            result,
                            tc.id.clone(),
                            tc.name.clone(),
                            tool_started.elapsed(),
                        ));
                    } else {
                        let call_id = tc.id.clone();
                        let name = tc.name.clone();
                        let args = tc.arguments.clone();
                        let destructive = is_destructive_tool(&name);
                        let scope = context.scope_snapshot.clone();
                        let sid = session_id.to_string();
                        let rid = context.run_id.clone();
                        let executor = self.tool_executor.clone();
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
                                        run_id: rid,
                                    },
                                )
                                .await;
                            (
                                call_id,
                                name,
                                args,
                                result,
                                start.elapsed().as_millis() as u64,
                            )
                        });
                        if destructive {
                            destructive_spawns.push((
                                i,
                                tc.id.clone(),
                                tc.name.clone(),
                                tc.arguments.clone(),
                                handle,
                            ));
                        } else {
                            read_spawns.push((
                                i,
                                tc.id.clone(),
                                tc.name.clone(),
                                tc.arguments.clone(),
                                handle,
                            ));
                        }
                    }
                }

                // ── Phase 2a: await read/info tools concurrently ──────
                for (_idx, call_id, tool_name, _tool_args, handle) in read_spawns {
                    match handle.await {
                        Ok((cid, name, _args, result, elapsed_ms)) => {
                            fast_results.push((
                                0,
                                result,
                                cid,
                                name,
                                std::time::Duration::from_millis(elapsed_ms),
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, tool = %tool_name, "parallel read tool panicked");
                            fast_results.push((
                                0,
                                super::tool_executor::ConversationToolResult {
                                    tool_call_id: call_id.clone(),
                                    tool_name: tool_name.clone(),
                                    content: serde_json::json!({"error": format!("{e}")})
                                        .to_string(),
                                    success: false,
                                },
                                call_id,
                                tool_name,
                                std::time::Duration::ZERO,
                            ));
                        }
                    }
                }

                // ── Phase 2b: await destructive tools sequentially ────
                for (_idx, call_id, tool_name, _tool_args, handle) in destructive_spawns {
                    match handle.await {
                        Ok((cid, name, _args, result, elapsed_ms)) => {
                            fast_results.push((
                                0,
                                result,
                                cid,
                                name,
                                std::time::Duration::from_millis(elapsed_ms),
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, tool = %tool_name, "destructive tool panicked");
                            fast_results.push((
                                0,
                                super::tool_executor::ConversationToolResult {
                                    tool_call_id: call_id.clone(),
                                    tool_name: tool_name.clone(),
                                    content: serde_json::json!({"error": format!("{e}")})
                                        .to_string(),
                                    success: false,
                                },
                                call_id,
                                tool_name,
                                std::time::Duration::ZERO,
                            ));
                        }
                    }
                }

                // ── Track unreachable targets for diagnostic ──
                let mut unreachable_targets: Vec<String> = Vec::new();

                // ── Phase 3: process all results (SSE + evidence + traffic + summary) ──
                for (idx, result, call_id, tool_name, elapsed) in &fast_results {
                    // Detect unreachable targets so the model doesn't retry dead hosts
                    if !result.success && is_unreachable_error(&result.content) {
                        if let Some(target) = extract_target_from_content(&result.content) {
                            let host = target_host_from_url(&target);
                            if !unreachable_targets.contains(&host) {
                                unreachable_targets.push(host);
                            }
                        }
                    }
                    let tool_args = output
                        .tool_calls
                        .get(*idx)
                        .map(|tc| tc.arguments.as_str())
                        .unwrap_or("");
                    // SSE: stream tool result as it arrives
                    let event_payload = serde_json::json!({
                        "turn_id": turn_id,
                        "tool_name": tool_name,
                        "tool_call_id": call_id,
                        "success": result.success,
                        "summary": result.content.chars().take(300).collect::<String>(),
                    });
                    self.events
                        .emit("conversation.tool.completed", event_payload.clone())
                        .await
                        .ok();
                    // NATS: publish for external subscribers (frontend SSE, monitoring)
                    if let Some(ref nats) = self.nats {
                        let payload = serde_json::to_vec(&event_payload).unwrap_or_default();
                        nats.publish(&format!("penta.conversation.tools.{}", turn_id), &payload)
                            .await;
                    }

                    let effective_observation =
                        result.success && has_effective_observation(tool_name, &result.content);
                    loop_engine.record_tool_result(result.success, effective_observation);
                    if let Some(diagnostic) = scan_progress.record_tool_result(
                        tool_name,
                        result.success,
                        effective_observation,
                        &result.content,
                    ) {
                        loop_diagnostic = Some(diagnostic);
                    }
                    if tool_name == "traffic_replay" && result.success {
                        scan_progress.record_traffic_replay();
                    }

                    let evidence_id = if is_evidence_tool(tool_name)
                        || is_traffic_tool(tool_name)
                        || is_finding_tool(tool_name)
                    {
                        None
                    } else if let Some(run_id) = context.run_id.as_deref() {
                        let eid = self
                            .repository
                            .record_tool_result(
                                run_id,
                                call_id,
                                tool_name,
                                &tool_args,
                                result.success,
                                &result.content,
                                elapsed.as_millis().try_into().unwrap_or(u64::MAX),
                            )
                            .await?;
                        evidence_ids.push(eid.clone());
                        scan_progress.record_evidence_tool(&eid, tool_name);
                        Some(eid)
                    } else {
                        None
                    };

                    if let Some(run_id) = context.run_id.as_deref()
                        && matches!(tool_name.as_str(), "http_request" | "http_probe")
                    {
                        self.repository
                            .record_http_traffic(
                                run_id,
                                call_id,
                                tool_name,
                                &tool_args,
                                &result.content,
                                None,
                            )
                            .await?;
                    }
                    append_run_summary(
                        &mut run_summary,
                        round + 1,
                        tool_name,
                        result.success,
                        evidence_id.as_deref(),
                        &result.content,
                    );

                    let display = serde_json::json!({"tool_name":tool_name,"tool_call_id":call_id,"success":result.success,"result":result.content});
                    self.repository
                        .write_intermediate_message(
                            turn_id,
                            session_id,
                            "tool",
                            &display.to_string(),
                            "tool_result",
                        )
                        .await?;
                    // Also write to pent_run_events so Run detail page shows progress
                    if let Some(ref run_id) = context.run_id {
                        self.repository
                            .record_run_event(run_id, "tool.executed", &display)
                            .await?;
                    }
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("tool_result:{}", call_id), role: "tool".into(),
                        content: serde_json::json!({"tool_call_id":call_id,"result":compact_model_observation(&result.content)}).to_string(),
                        format: "tool_result".into(),
                    });
                }
                if stopped_by_budget.is_some() {
                    break;
                }
                context.run_summary = run_summary.clone();
                let loop_checkpoint = loop_engine.checkpoint();
                context.policy_snapshot["_checkpoint"] = serde_json::json!({
                    "round": loop_checkpoint.next_round,
                    "request_count": loop_checkpoint.request_count,
                    "token_count": loop_checkpoint.token_count,
                    "successful_observations": loop_checkpoint.effective_observations,
                    "orchestration": format!("{:?}", orchestration).to_ascii_lowercase(),
                    "updated_at": chrono::Utc::now()
                });

                // ── RMOS: update memory from tool results ──
                for (_idx, result, _call_id, tool_name, _elapsed) in &fast_results {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result.content) {
                        pentest_memory.absorb_tool_result(tool_name, &parsed);
                    }
                }
                // Generate next_actions for uncovered methodology items
                pentest_memory.ensure_coverage_actions();

                self.repository
                    .checkpoint_context(turn_id, &context, &run_summary, &evidence_ids)
                    .await?;

                if scan_progress.stall_exhausted() && !stall_terminal_prompted {
                    stall_terminal_prompted = true;
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_stall_exhausted:round_{round}"),
                        role: "user".into(),
                        content: "运行时报告：连续多次工具调用失败且没有产生新 Evidence。\
                                  请判断失败原因并立即改变工具或参数；如果没有可行替代路径，\
                                  返回 NO_PATH 并说明具体阻塞，不要继续机械重试。"
                            .into(),
                        format: "text".into(),
                    });
                }
                if let Some(diagnostic) = loop_diagnostic {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_diagnostic:round_{round}"),
                        role: "user".into(),
                        content: diagnostic,
                        format: "text".into(),
                    });
                }
                // Tell model about unreachable targets so it doesn't waste rounds retrying
                for host in &unreachable_targets {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_unreachable:{}", host),
                        role: "user".into(),
                        content: format!(
                            "目标 {host} 不可达（连接被拒绝或超时），请**跳过**该目标，\
                             不要再次调用 http_request/http_probe 指向它。\
                             如果所有目标都不可达请输出 NO_PATH。"
                        ),
                        format: "text".into(),
                    });
                }
                // Remove repeatedly-failing tools from model catalog so the
                // agent is forced to try alternatives instead of retrying.
                let exhausted = scan_progress.exhausted_tools().clone();
                if !exhausted.is_empty() {
                    self.model_gateway.set_exhausted_tools(exhausted);
                }
                continue;
            }

            // A tool-free response must declare, or be treated as, a lifecycle
            // action. The runtime validates the action against actual progress
            // and Evidence rather than forcing a fixed pentest phase template.
            match parse_agent_directive(&output.content) {
                AgentDirective::Replan(reason) => {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("assistant_replan:round_{round}"),
                        role: "assistant".into(),
                        content: format!("REPLAN: {reason}"),
                        format: "text".into(),
                    });
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_replan:round_{round}"),
                        role: "user".into(),
                        content: "请根据新计划直接选择并调用下一项最有信息增益的可用工具；\
                                  不要只输出另一份计划。"
                            .into(),
                        format: "text".into(),
                    });
                    continue;
                }
                AgentDirective::NoPath(reason) => {
                    if let Some(rejection) = scan_progress.no_path_rejection() {
                        push_gate_rejection(
                            &mut context,
                            round,
                            &output.content,
                            &format!("NO_PATH 被拒绝：{rejection}"),
                        );
                        continue;
                    }
                    output.content = format!(
                        "当前授权范围和可用能力下无法继续：{}",
                        nonempty_or(&reason, "模型未提供具体原因")
                    );
                }
                AgentDirective::AskUser(question) => {
                    // Autonomous mode: treat ASK_USER as a signal to replan.
                    // The model has exhausted its current approach — push it to try
                    // alternative tools or conclude with NO_PATH.
                    let reason = nonempty_or(&question, "模型判断需要补充信息，但当前为全自主模式");
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("assistant_askuser_replan:round_{round}"),
                        role: "assistant".into(),
                        content: format!("ASK_USER: {reason}"),
                        format: "text".into(),
                    });
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_askuser_replan:round_{round}"),
                        role: "user".into(),
                        content: format!(
                            "你的 ASK_USER（{reason}）在全自主模式下不可用。\
                             请改为：1) 换其他工具或策略继续验证；\
                             2) 如果确实无路可走，输出 NO_PATH 说明阻塞点。"
                        ),
                        format: "text".into(),
                    });
                    continue;
                }
                AgentDirective::Final(candidate) | AgentDirective::Candidate(candidate) => {
                    if let Some(rejection) =
                        scan_progress.final_rejection(&candidate, &evidence_ids)
                    {
                        push_gate_rejection(
                            &mut context,
                            round,
                            &output.content,
                            &format!("FINAL 被拒绝：{rejection}"),
                        );
                        continue;
                    }
                    if let Some(rejection) = self
                        .claim_projection_rejection(
                            context.run_id.as_deref(),
                            &candidate,
                            &evidence_ids,
                        )
                        .await?
                    {
                        push_gate_rejection(
                            &mut context,
                            round,
                            &output.content,
                            &format!("FINAL 被拒绝：{rejection}"),
                        );
                        continue;
                    }
                    if let Some(rejection) =
                        scan_progress.maturity_rejection(&candidate, &evidence_ids)
                    {
                        push_gate_rejection(
                            &mut context,
                            round,
                            &output.content,
                            &format!("FINAL 被拒绝：{rejection}"),
                        );
                        continue;
                    }
                    output.content = candidate;
                    // ── RMOS: don't stop if there are high-priority next actions ──
                    // Cap P1/P2 pending items at 3 to prevent infinite loops when
                    // the target simply doesn't have those vulnerability types.
                    let pending: Vec<_> = pentest_memory
                        .next_actions
                        .iter()
                        .filter(|a| a.priority <= 2)
                        .take(3)
                        .collect();
                    if !pending.is_empty() {
                        let mut lines = vec![
                            "## ⚠ 渗透测试尚未完成\n".to_string(),
                            "以下高优先级任务未执行，请选择一个继续操作：".to_string(),
                        ];
                        for a in pending.iter().take(5) {
                            let tool = a.tool.as_deref().unwrap_or("manual");
                            lines.push(format!(
                                "- [P{}] {} — `{}` {}",
                                a.priority, a.action, tool, a.target
                            ));
                        }
                        lines.push("\n选择最优先的任务，直接调用工具继续渗透。".to_string());
                        push_gate_rejection(
                            &mut context,
                            round,
                            &output.content,
                            &lines.join("\n"),
                        );
                        continue;
                    }
                }
            }
            final_output = Some(output);
            finished_normally = true;
            break;
        }

        if !finished_normally {
            let stop_reason = stopped_by_budget.unwrap_or("rounds");
            let loop_checkpoint = loop_engine.checkpoint();
            context.recent_messages.push(AgentMessage {
                message_id: "runtime_budget_finalization".into(),
                role: "user".into(),
                content: budget_finalization_prompt(
                    stop_reason,
                    loop_engine.rounds_used(),
                    loop_checkpoint.request_count,
                    loop_checkpoint.token_count,
                    &run_summary,
                    &evidence_ids,
                ),
                format: "text".into(),
            });
            match self.model_gateway.generate(&context, intent).await {
                Ok(output) if !output.content.trim().is_empty() => {
                    final_output = Some(ModelTurnOutput {
                        content: normalize_budget_finalization(&output.content, stop_reason),
                        ..output
                    });
                }
                Ok(_) => {
                    tracing::warn!(stop_reason, "budget finalization returned empty content");
                }
                Err(error) => {
                    tracing::warn!(
                        stop_reason,
                        error = %error,
                        "budget finalization model call failed"
                    );
                }
            }
            if final_output.is_none() {
                final_output = Some(ModelTurnOutput {
                    content: fallback_budget_summary(stop_reason, &run_summary, &evidence_ids),
                    reasoning_summary: None,
                    plan_update: None,
                    citations: vec![],
                    usage: Default::default(),
                    tool_calls: vec![],
                });
            }
        }
        let output = final_output.ok_or_else(|| anyhow::anyhow!("模型未返回有效响应"))?;
        let loop_checkpoint = loop_engine.checkpoint();

        let final_content = if context.run_id.is_some() {
            let report = if output.content.trim().is_empty() {
                "本轮未生成文字结论；请依据 Evidence 进行复核。"
            } else {
                output.content.trim()
            };
            let run_reference = context.run_id.as_deref().unwrap_or("无");
            format!(
                "{report}\n\n---\n\n## 结构化证据索引\n\n\
                 - 工具调用：{}\n- 有效观察：{}\n\
                 - Evidence 数量：{}\n- Run：`{run_reference}`\n\n\
                 各项发现应引用其 Evidence；完整证据可在该 Run 的 Evidence 页面复核。\
                 原始响应未直接拼接进报告。",
                loop_checkpoint.request_count,
                loop_checkpoint.effective_observations,
                evidence_ids.len()
            )
        } else if !output.content.trim().is_empty() {
            output.content.clone()
        } else {
            format!(
                "本轮完成了 {} 次受控工具调用并形成 {} 条有效观察。\
                 原始响应已保存为 Evidence，报告不内联原始输出。",
                loop_checkpoint.request_count, loop_checkpoint.effective_observations
            )
        };

        // Clean up any still-running sub-agents before returning
        agent_session.abort_all().await;

        Ok(final_content)
    }

    /// Execute a fast inline tool (knowledge, skill, traffic, evidence) synchronously.
    async fn execute_fast_tool(
        &self,
        tc: &super::ports::ModelToolCall,
        session_id: &str,
        context: &ConversationContext,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        match tc.name.as_str() {
            "knowledge_search" => {
                self.execute_knowledge_search(
                    &tc.id,
                    &tc.arguments,
                    session_id,
                    context.run_id.as_deref(),
                )
                .await
            }
            "vision_analyze" => {
                self.execute_vision_analyze(&tc.id, &tc.arguments, context.run_id.as_deref())
                    .await
            }
            "skill_list" | "skill_load" => {
                let content = self
                    .repository
                    .execute_skill_tool(&tc.name, &tc.arguments)
                    .await
                    .map(|v| v.to_string());
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| {
                        serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            "agent_spawn" => {
                self.execute_agent_spawn(&tc.id, &tc.arguments, session_id, context, agent_session)
                    .await
            }
            "agent_status" => {
                self.execute_agent_status(&tc.id, &tc.arguments, agent_session)
                    .await
            }
            "agent_collect" => {
                self.execute_agent_collect(&tc.id, &tc.arguments, agent_session)
                    .await
            }
            "agent_list" => self.execute_agent_list(&tc.id, agent_session).await,
            "plan_create" | "plan_get" | "plan_update" => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => {
                        super::plan_board::execute(&self.pool, run_id, &tc.name, &tc.arguments)
                            .await
                    }
                    None => Err(anyhow::anyhow!("计划工具只能在已有 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content
                        .map(|value| value.to_string())
                        .unwrap_or_else(|error| {
                            serde_json::json!({"error":error.to_string(),"retryable":false})
                                .to_string()
                        }),
                }
            }
            "record_finding" => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => {
                        super::run_bridge::record_finding(&self.pool, run_id, &tc.arguments)
                            .await
                            .map(|value| value.to_string())
                    }
                    None => Err("record_finding 只能在已有执行 Run 中使用".to_string()),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|error| {
                        serde_json::json!({"error":error,"retryable":false}).to_string()
                    }),
                }
            }
            name if is_traffic_tool(name) => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => {
                        self.execute_traffic_tool(
                            run_id,
                            session_id,
                            name,
                            &tc.id,
                            &tc.arguments,
                            &context.scope_snapshot,
                        )
                        .await
                    }
                    None => Err(anyhow::anyhow!("流量工具只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| {
                        serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            name if is_evidence_tool(name) => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => self
                        .repository
                        .execute_evidence_tool(run_id, name, &tc.arguments)
                        .await
                        .map(|v| v.to_string()),
                    None => Err(anyhow::anyhow!("Evidence 工具只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| {
                        serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            _ => {
                self.tool_executor
                    .execute(
                        &RuntimeToolCall {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        },
                        &ToolExecutionContext {
                            scope_snapshot: context.scope_snapshot.clone(),
                            session_id: session_id.to_string(),
                            run_id: context.run_id.clone(),
                        },
                    )
                    .await
            }
        }
    }

    /// Emit event to SSE sink and NATS (if connected).
    async fn emit_event(&self, event: &str, payload: serde_json::Value) {
        self.events.emit(event, payload.clone()).await.ok();
        if let Some(ref nats) = self.nats {
            let bytes = serde_json::to_vec(&payload).unwrap_or_default();
            nats.publish(&format!("penta.conversation.{}", event), &bytes)
                .await;
        }
    }

    async fn execute_knowledge_search(
        &self,
        tool_call_id: &str,
        arguments: &str,
        session_id: &str,
        run_id: Option<&str>,
    ) -> super::tool_executor::ConversationToolResult {
        let parsed = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
        let query = parsed
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if query.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "knowledge_search".into(),
                content: serde_json::json!({"error":"query 不能为空"}).to_string(),
                success: false,
            };
        }
        // ── M3 + D-07: multi-query hybrid search + deterministic rerank ──
        let multi_query_max = std::env::var("PENTA_MULTI_QUERY_MAX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(4)
            .clamp(1, 8);
        let queries = crate::tooling::reranker::knowledge::expand_queries(query, multi_query_max);
        let mut guides: Vec<super::ports::KnowledgeGuide> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for variant in &queries {
            if let Ok(mut result) = self.knowledge_source.search_guides(variant, 3).await {
                for guide in result.drain(..) {
                    let key = format!("{}|{}", guide.id, guide.title);
                    if seen.insert(key) {
                        guides.push(guide);
                    }
                }
            }
        }
        if let Ok(memories) = search_vector_memories(&self.pool, query, 3).await {
            for memory in memories {
                guides.push(super::ports::KnowledgeGuide {
                    id: format!("vec-{}", uuid::Uuid::new_v4()),
                    title: "向量记忆匹配".into(),
                    source: Some("pgvector".into()),
                    content: memory,
                });
            }
        }
        // Rerank by term overlap against the original query (title 3x body).
        guides.sort_by(|left, right| {
            crate::tooling::reranker::knowledge::score_guide(query, &right.title, &right.content)
                .partial_cmp(&crate::tooling::reranker::knowledge::score_guide(
                    query,
                    &left.title,
                    &left.content,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        guides.truncate(5);
        let success = !guides.is_empty();
        if success {
            self.log_knowledge_retrieval(query, &guides, session_id, run_id)
                .await;
        }
        super::tool_executor::ConversationToolResult {
            tool_call_id: tool_call_id.to_string(),
            tool_name: "knowledge_search".into(),
            content: serde_json::json!({
                "query": query,
                "guide_count": guides.len(),
                "guides": guides,
                "instruction": "根据 Wiki 方法和向量记忆选择当前可用工具继续验证。"
            })
            .to_string(),
            success,
        }
    }

    /// Vision analysis (D-10): analyze a screenshot/data URL with a vision
    /// model and return only the text summary. The image never enters the
    /// main agent context; the summary is persisted as evidence upstream.
    async fn execute_vision_analyze(
        &self,
        tool_call_id: &str,
        arguments: &str,
        _run_id: Option<&str>,
    ) -> super::tool_executor::ConversationToolResult {
        let parsed = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
        let image = parsed
            .get("image_data_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if image.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({"error": "image_data_url 不能为空"}).to_string(),
                success: false,
            };
        }
        if !image.starts_with("data:image/") {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({
                    "error": "image_data_url 必须是 data:image/...;base64, 数据 URL"
                })
                .to_string(),
                success: false,
            };
        }
        let question = parsed.get("question").and_then(serde_json::Value::as_str);
        let prompt = crate::infrastructure::vision::screenshot_prompt(question);
        match crate::infrastructure::vision::analyze_image(&self.pool, image, &prompt).await {
            Ok(summary) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({
                    "summary": summary,
                    "instruction": "视觉摘要仅作辅助线索，需用当前可用工具继续验证。"
                })
                .to_string(),
                success: true,
            },
            Err(error) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({"error": error, "retryable": false}).to_string(),
                success: false,
            },
        }
    }

    /// Best-effort audit log for knowledge retrieval; failures only warn.
    async fn log_knowledge_retrieval(
        &self,
        query: &str,
        guides: &[super::ports::KnowledgeGuide],
        session_id: &str,
        run_id: Option<&str>,
    ) {
        let retrieved_items: Vec<serde_json::Value> = guides
            .iter()
            .map(|guide| {
                serde_json::json!({
                    "id": if guide.id.is_empty() { guide.title.as_str() } else { guide.id.as_str() },
                    "title": guide.title,
                })
            })
            .collect();
        let result = async {
            let tenant_id: i64 = sqlx::query_scalar(
                "SELECT COALESCE(
                     (SELECT tenant_id FROM pent_runs WHERE id=$1),
                     (SELECT tenant_id FROM pent_agent_sessions WHERE id=$2),
                     0
                 )",
            )
            .bind(run_id)
            .bind(session_id)
            .fetch_one(&self.pool)
            .await?;
            sqlx::query(
                "INSERT INTO pent_knowledge_retrieval_logs(
                     id, tenant_id, query, risk_type, conversation_id, message_id, retrieved_items
                 ) VALUES($1,$2,$3,'',$4,$5,$6)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(tenant_id)
            .bind(query)
            .bind(session_id)
            .bind(Option::<String>::None)
            .bind(serde_json::json!(retrieved_items))
            .execute(&self.pool)
            .await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(%error, query, "knowledge retrieval log write failed");
        }
    }

    /// Verify that quotable assertions in a verified FINAL are verbatim-
    /// projectable into the run's cited evidence. Returns the model-facing
    /// rejection reason when fabrication is detected.
    async fn claim_projection_rejection(
        &self,
        run_id: Option<&str>,
        candidate: &str,
        evidence_ids: &[String],
    ) -> anyhow::Result<Option<String>> {
        let Some(run_id) = run_id else {
            return Ok(None);
        };
        if !super::scan_progress::has_verified_claim(candidate) {
            return Ok(None);
        }
        let cited: Vec<String> = evidence_ids
            .iter()
            .filter(|id| candidate.contains(id.as_str()))
            .cloned()
            .collect();
        if cited.is_empty() {
            // The citation gate in `final_rejection` already rejects this case.
            return Ok(None);
        }
        let texts = self.repository.evidence_texts(run_id, &cited).await?;
        Ok(super::claim_gate::assess_claims(candidate, &texts).rejection())
    }

    // ── Agent parallelism tool handlers ────────────────────────────────

    async fn execute_agent_spawn(
        &self,
        tool_call_id: &str,
        arguments: &str,
        session_id: &str,
        context: &ConversationContext,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let args: serde_json::Value = match serde_json::from_str(arguments) {
            Ok(v) => v,
            Err(e) => {
                return super::tool_executor::ConversationToolResult {
                    tool_call_id: tool_call_id.to_string(),
                    tool_name: "agent_spawn".into(),
                    content: serde_json::json!({"error": format!("参数解析失败: {e}")}).to_string(),
                    success: false,
                };
            }
        };
        let description = args
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("未命名子任务")
            .to_string();
        let mut prompt = args
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&description)
            .to_string();
        let role = args
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if let Ok(Some(profile)) = super::agent_profiles::load(role) {
            prompt = format!(
                "## 专家角色：{} ({})\n{}\n\n## 本次委派\n{}",
                profile.name, profile.id, profile.instruction, prompt
            );
        }
        if prompt.trim().is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({"error": "prompt 不能为空"}).to_string(),
                success: false,
            };
        }

        let target = args
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        let active = agent_session.active_count().await;
        let max_concurrent = std::env::var("PENTA_MAX_AGENTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4usize);
        if active >= max_concurrent {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({
                    "error": format!("已达到最大并发子代理数 ({})，当前活跃 {} 个，请等待部分完成后再创建", max_concurrent, active),
                    "active_agents": active,
                    "max_concurrent": max_concurrent,
                })
                .to_string(),
                success: false,
            };
        }

        match agent_session
            .spawn(AgentSpawnRequest {
                description: description.clone(),
                prompt: prompt.clone(),
                session_id: session_id.to_string(),
                parent_run_id: context.run_id.clone().unwrap_or_else(|| "unknown".into()),
                target,
            })
            .await
        {
            Ok(agent_id) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({
                    "agent_id": agent_id,
                    "description": description,
                    "status": "running",
                    "active_agents": active + 1,
                    "max_concurrent": max_concurrent,
                    "hint": format!("子代理 {} 已启动，稍后使用 agent_collect 获取结果", agent_id),
                })
                .to_string(),
                success: true,
            },
            Err(e) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({"error": e.to_string()}).to_string(),
                success: false,
            },
        }
    }

    async fn execute_agent_status(
        &self,
        tool_call_id: &str,
        arguments: &str,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let agent_id = args
            .get("agent_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        if agent_id.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_status".into(),
                content: serde_json::json!({"error": "agent_id 不能为空"}).to_string(),
                success: false,
            };
        }

        match agent_session.status(agent_id).await {
            Some(agent) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_status".into(),
                content: serde_json::to_value(&agent).unwrap_or_default().to_string(),
                success: true,
            },
            None => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_status".into(),
                content: serde_json::json!({"error": format!("子代理 {} 不存在", agent_id)})
                    .to_string(),
                success: false,
            },
        }
    }

    async fn execute_agent_collect(
        &self,
        tool_call_id: &str,
        arguments: &str,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let agent_id = args
            .get("agent_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        if agent_id.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({"error": "agent_id 不能为空"}).to_string(),
                success: false,
            };
        }

        // Check if agent exists
        if agent_session.status(agent_id).await.is_none() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({"error": format!("子代理 {} 不存在", agent_id)})
                    .to_string(),
                success: false,
            };
        }

        match agent_session.collect(agent_id).await {
            Some(result) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({
                    "agent_id": agent_id,
                    "result": result,
                })
                .to_string(),
                success: !result.starts_with("子代理执行失败") && !result.starts_with("等待"),
            },
            None => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({
                    "agent_id": agent_id,
                    "error": "无法获取子代理结果",
                })
                .to_string(),
                success: false,
            },
        }
    }

    async fn execute_agent_list(
        &self,
        tool_call_id: &str,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let agents = agent_session.list().await;
        super::tool_executor::ConversationToolResult {
            tool_call_id: tool_call_id.to_string(),
            tool_name: "agent_list".into(),
            content: serde_json::json!({
                "agents": agents,
                "count": agents.len(),
                "active": agents.iter().filter(|a| a.status == RuntimeAgentStatus::Running).count(),
            })
            .to_string(),
            success: true,
        }
    }

    /// Execute one of the traffic tools. Read-only tools delegate to the
    /// repository; `traffic_replay` re-issues the stored request through the
    /// isolated HTTP executor (scope guard applies) and records the new
    /// exchange linked to the original traffic row.
    async fn execute_traffic_tool(
        &self,
        run_id: &str,
        session_id: &str,
        tool_name: &str,
        tool_call_id: &str,
        arguments: &str,
        scope_snapshot: &serde_json::Value,
    ) -> anyhow::Result<String> {
        if tool_name != "traffic_replay" {
            return self
                .repository
                .execute_traffic_tool(run_id, tool_name, arguments)
                .await
                .map(|value| value.to_string());
        }
        let arguments_value = serde_json::from_str::<serde_json::Value>(arguments)?;
        let traffic_id = arguments_value
            .get("traffic_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("traffic_replay 缺少 traffic_id"))?;
        let replay_args = self
            .repository
            .load_traffic_request(run_id, traffic_id)
            .await?;
        let result = self
            .tool_executor
            .execute(
                &RuntimeToolCall {
                    id: tool_call_id.to_string(),
                    name: "http_request".into(),
                    arguments: replay_args.to_string(),
                },
                &ToolExecutionContext {
                    scope_snapshot: scope_snapshot.clone(),
                    session_id: session_id.to_string(),
                    run_id: Some(run_id.to_string()),
                },
            )
            .await;
        let traffic_ids = self
            .repository
            .record_http_traffic(
                run_id,
                tool_call_id,
                "http_request",
                &replay_args.to_string(),
                &result.content,
                Some(traffic_id),
            )
            .await?;
        let mut content = serde_json::from_str::<serde_json::Value>(&result.content)
            .unwrap_or_else(|_| serde_json::json!({"raw": result.content}));
        if let Some(object) = content.as_object_mut() {
            object.insert("replay_of".into(), serde_json::json!(traffic_id));
            object.insert("replay_traffic_ids".into(), serde_json::json!(traffic_ids));
        }
        Ok(content.to_string())
    }
}

fn append_run_summary(
    summary: &mut String,
    round: usize,
    tool_name: &str,
    success: bool,
    evidence_id: Option<&str>,
    observation: &str,
) {
    let observation = serde_json::from_str::<serde_json::Value>(observation)
        .ok()
        .map(|value| {
            for key in [
                "status",
                "status_code",
                "reachable",
                "url",
                "target",
                "findings",
                "endpoints",
                "security_hints",
            ] {
                if let Some(item) = value.get(key) {
                    return format!("{key}={}", compact_summary_value(item));
                }
            }
            "结构化观察已保存".to_string()
        })
        .unwrap_or_else(|| "非结构化观察已保存".to_string());
    let line = format!(
        "\n- 第{round}轮 `{tool_name}`：{}；{}；Evidence `{}`",
        if success { "成功" } else { "失败" },
        observation,
        evidence_id.unwrap_or("无")
    );
    summary.push_str(&line);
    if summary.chars().count() > MAX_RUN_SUMMARY_CHARS {
        *summary = summary
            .chars()
            .rev()
            .take(MAX_RUN_SUMMARY_CHARS)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        summary.insert_str(0, "[较早摘要已滚动压缩]\n");
    }
}

fn compact_summary_value(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 300 {
        text
    } else {
        format!("{}…", text.chars().take(300).collect::<String>())
    }
}

fn compact_model_observation(content: &str) -> String {
    if content.chars().count() <= MAX_MODEL_TOOL_OBSERVATION_CHARS {
        return content.to_string();
    }
    let prefix: String = content
        .chars()
        .take(MAX_MODEL_TOOL_OBSERVATION_CHARS)
        .collect();
    format!(
        "{prefix}\n...[模型上下文已截断；完整原始响应已保存为 Evidence，共 {} 字符]",
        content.chars().count()
    )
}

fn push_gate_rejection(
    context: &mut ConversationContext,
    round: usize,
    candidate: &str,
    rejection: &str,
) {
    context.recent_messages.push(AgentMessage {
        message_id: format!("assistant_gate_candidate:round_{round}"),
        role: "assistant".into(),
        content: candidate.to_string(),
        format: "text".into(),
    });
    context.recent_messages.push(AgentMessage {
        message_id: format!("runtime_gate:round_{round}"),
        role: "user".into(),
        content: format!(
            "自主循环完成闸门：{rejection}\n\
             请自行选择下一步：调用工具补充证据、REPLAN、ASK_USER，或在路径确实耗尽时\
             给出有依据的 NO_PATH。"
        ),
        format: "text".into(),
    });
}

fn budget_finalization_prompt(
    stop_reason: &str,
    rounds: usize,
    requests: usize,
    tokens: u64,
    run_summary: &str,
    evidence_ids: &[String],
) -> String {
    format!(
        "## 运行时预算收尾\n\n\
         已触发 `{stop_reason}` 预算边界（轮次 {rounds}、请求 {requests}、累计 Token {tokens}）。\
         本次调用不再提供工具。请根据已有摘要和 Evidence 输出一次最终判断：\n\
         - 已有足够证据：输出 `FINAL:`，明确区分已验证事实与未验证线索，并引用 Evidence ID；\n\
         - 证据不足或路径耗尽：输出 `NO_PATH:`，说明阻塞和建议的后续动作；\n\
         - 必须由用户补充信息：输出 `ASK_USER:`。\n\n\
         不要请求或调用工具，不要声称任务完整完成。\n\n\
         ## 已有运行摘要\n{}\n\n## Evidence\n{}",
        nonempty_or(run_summary, "尚无可用运行摘要"),
        if evidence_ids.is_empty() {
            "无".to_string()
        } else {
            evidence_ids.join("、")
        }
    )
}

fn normalize_budget_finalization(content: &str, stop_reason: &str) -> String {
    let conclusion = match parse_agent_directive(content) {
        AgentDirective::Final(value)
        | AgentDirective::NoPath(value)
        | AgentDirective::AskUser(value)
        | AgentDirective::Candidate(value)
        | AgentDirective::Replan(value) => value,
    };
    format!(
        "本次运行因 `{stop_reason}` 预算边界停止，以下为基于当前 Evidence 的阶段性结论：\n\n{}",
        nonempty_or(&conclusion, "模型未生成可用的阶段性结论")
    )
}

fn fallback_budget_summary(
    stop_reason: &str,
    run_summary: &str,
    evidence_ids: &[String],
) -> String {
    format!(
        "本次运行因 `{stop_reason}` 预算边界停止，终局模型调用未返回可用内容。\
         以下内容仅代表已保存的阶段性进展，不表示测试完整完成。\n\n{}\n\nEvidence：{}",
        nonempty_or(run_summary, "尚无可用运行摘要。"),
        if evidence_ids.is_empty() {
            "无".to_string()
        } else {
            evidence_ids.join("、")
        }
    )
}

fn nonempty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

/// M3: Vector semantic search on pent_knowledge_documents via pgvector.
/// Falls back gracefully if no embedding provider is configured.
async fn search_vector_memories(
    pool: &kairos_platform_framework_database::PgPool,
    query: &str,
    top_k: usize,
) -> anyhow::Result<Vec<String>> {
    let mut embedder = crate::infrastructure::embedding::EmbeddingProvider::new(pool.clone());
    if !embedder.try_configure().await.unwrap_or(false) {
        return Ok(Vec::new());
    }
    let Some(embedding) = embedder.embed(query).await else {
        return Ok(Vec::new());
    };
    let index = crate::infrastructure::vector::PgVectorIndex::new(pool.clone());
    let results = index
        .search(&embedding, None, top_k)
        .await
        .unwrap_or_default();
    Ok(results
        .into_iter()
        .map(|r| format!("{} v{} (score: {:.2})", r.tool_id, r.version, r.score))
        .collect())
}

fn compact_runtime_error(error: &str) -> String {
    const MAX_ERROR_CHARS: usize = 500;
    let compact = error.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_ERROR_CHARS {
        compact
    } else {
        format!(
            "{}…",
            compact.chars().take(MAX_ERROR_CHARS).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentDirective, MAX_MODEL_TOOL_OBSERVATION_CHARS, compact_model_observation,
        compact_runtime_error, has_effective_observation, parse_agent_directive,
    };

    #[test]
    fn runtime_error_is_compacted_before_returning_to_model() {
        let error = format!("secret-like detail {}", "x".repeat(1_000));
        let compact = compact_runtime_error(&error);
        assert!(compact.chars().count() <= 501);
        assert!(compact.ends_with('…'));
    }

    #[test]
    fn empty_placeholder_js_is_not_an_effective_observation() {
        assert!(!has_effective_observation(
            "js_analyze",
            r#"{"endpoints":[],"security_hints":[],"technology_hints":[]}"#
        ));
        assert!(has_effective_observation(
            "js_analyze",
            r#"{"endpoints":["/api/users"],"security_hints":[]}"#
        ));
    }

    #[test]
    fn blocked_results_are_not_effective() {
        assert!(!has_effective_observation(
            "dir_fuzz",
            r#"{"blocked":true,"error":"disabled"}"#
        ));
    }

    #[test]
    fn knowledge_guides_do_not_count_as_target_evidence() {
        assert!(!has_effective_observation(
            "knowledge_search",
            r#"{"guide_count":1,"guides":[{"title":"SQL injection"}]}"#
        ));
    }

    #[test]
    fn compacts_tool_observation_for_model_context() {
        let raw = "a".repeat(MAX_MODEL_TOOL_OBSERVATION_CHARS + 10);
        let compact = compact_model_observation(&raw);
        assert!(compact.contains("完整原始响应已保存为 Evidence"));
        assert!(compact.len() < raw.len() + 100);
    }

    #[test]
    fn parses_explicit_agent_lifecycle_actions() {
        assert_eq!(
            parse_agent_directive("FINAL: 结论"),
            AgentDirective::Final("结论".into())
        );
        assert_eq!(
            parse_agent_directive("ASK_USER: 请提供测试账号"),
            AgentDirective::AskUser("请提供测试账号".into())
        );
        assert_eq!(
            parse_agent_directive("NO_PATH: 缺少浏览器能力"),
            AgentDirective::NoPath("缺少浏览器能力".into())
        );
        assert_eq!(
            parse_agent_directive("REPLAN: 改用响应差异验证"),
            AgentDirective::Replan("改用响应差异验证".into())
        );
    }

    #[test]
    fn parses_json_action_protocol() {
        assert_eq!(
            parse_agent_directive(r#"{"action":"final","content":"目标已完成，证据 ev-1"}"#),
            AgentDirective::Final("目标已完成，证据 ev-1".into())
        );
        assert_eq!(
            parse_agent_directive(r#"{"action":"ASK_USER","content":"请提供目标 URL"}"#),
            AgentDirective::AskUser("请提供目标 URL".into())
        );
        assert_eq!(
            parse_agent_directive(r#"{"action":"no_path","content":"当前授权不支持该操作"}"#),
            AgentDirective::NoPath("当前授权不支持该操作".into())
        );
        assert_eq!(
            parse_agent_directive(r#"{"action":"replan","content":"改用目录扫描验证"}"#),
            AgentDirective::Replan("改用目录扫描验证".into())
        );
    }

    #[test]
    fn json_action_without_content_falls_back_to_raw() {
        assert_eq!(
            parse_agent_directive(r#"{"action":"final"}"#),
            AgentDirective::Final(r#"{"action":"final"}"#.to_string())
        );
    }

    #[test]
    fn plain_text_without_marker_is_candidate_final() {
        let directive = parse_agent_directive("目标似乎没有开放端口");
        assert_eq!(
            directive,
            AgentDirective::Candidate("目标似乎没有开放端口".into())
        );
    }

    #[test]
    fn chinese_markers_are_recognized() {
        assert_eq!(
            parse_agent_directive("最终结果：目标安全"),
            AgentDirective::Final("目标安全".into())
        );
        assert_eq!(
            parse_agent_directive("需要用户：请提供 API 密钥"),
            AgentDirective::AskUser("请提供 API 密钥".into())
        );
        assert_eq!(
            parse_agent_directive("无法继续：防火墙拦截所有请求"),
            AgentDirective::NoPath("防火墙拦截所有请求".into())
        );
        assert_eq!(
            parse_agent_directive("重新规划：改用 POST 请求测试"),
            AgentDirective::Replan("改用 POST 请求测试".into())
        );
    }
}

fn has_effective_observation(tool_name: &str, content: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return !content.trim().is_empty();
    };
    if value.get("blocked").and_then(serde_json::Value::as_bool) == Some(true)
        || value.get("error").is_some()
    {
        return false;
    }
    match tool_name {
        // Knowledge is methodology, not target evidence. It may guide the next
        // action but cannot by itself satisfy a security completion gate.
        "knowledge_search" | "evidence_list" | "evidence_search" | "evidence_view" => false,
        "js_analyze" => ["endpoints", "security_hints", "technology_hints"]
            .iter()
            .any(|key| {
                value
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|items| !items.is_empty())
            }),
        "http_probe" => value
            .get("results")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|results| {
                results.iter().any(|result| {
                    result.get("status").is_some()
                        || result.get("reachable").and_then(serde_json::Value::as_bool)
                            == Some(true)
                })
            }),
        "dns_lookup" => value
            .pointer("/records/A")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|records| !records.is_empty()),
        _ => true,
    }
}

fn is_evidence_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "evidence_list" | "evidence_search" | "evidence_view"
    )
}

fn is_traffic_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "traffic_list" | "traffic_view" | "traffic_diff" | "traffic_replay" | "traffic_sitemap"
    )
}

fn is_finding_tool(tool_name: &str) -> bool {
    tool_name == "record_finding"
}

fn is_skill_tool(tool_name: &str) -> bool {
    matches!(tool_name, "skill_list" | "skill_load")
}

fn is_reference_tool(tool_name: &str) -> bool {
    tool_name == "knowledge_search"
        || is_evidence_tool(tool_name)
        || is_finding_tool(tool_name)
        || is_skill_tool(tool_name)
}

/// Tools that execute inline (knowledge, evidence, traffic, skill, agent).
fn is_inline_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "knowledge_search"
            | "evidence_list"
            | "evidence_search"
            | "evidence_view"
            | "record_finding"
            | "traffic_list"
            | "traffic_view"
            | "traffic_sitemap"
            | "traffic_diff"
            | "traffic_replay"
            | "skill_list"
            | "skill_load"
            | "agent_spawn"
            | "agent_status"
            | "agent_collect"
            | "agent_list"
            | "plan_create"
            | "plan_get"
            | "plan_update"
            | "read"
            | "write"
            | "glob"
            | "grep"
            | "bash"
            | "web_search"
    )
}

/// Destructive tools that modify target state — must run sequentially.
/// These include vulnerability scanners, injection tools, and exploit tools.
fn is_destructive_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        // Vulnerability scanners
        "nuclei" | "web_vuln_scan"
            | "sqli_validate" | "xss_validate" | "csrf_validate"
            | "command_injection_validate" | "file_inclusion_validate"
            | "file_upload_validate"
            | "afrog" | "wapiti" | "skipfish"
            // Injection tools
            | "sqlmap" | "sqlmap_scan" | "xsser" | "dalfox" | "web_xss_scan"
            | "commix"
            // Brute force / fuzzing (high traffic)
            | "ffuf" | "wfuzz" | "gobuster" | "feroxbuster" | "dirb" | "dirsearch" | "dir_fuzz"
            // Exploit
            | "ssh_exec"
    )
}

/// Detect "target unreachable" errors in tool output.
fn is_unreachable_error(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("could not connect")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("no route to host")
        || lower.contains("failed to connect")
        || lower.contains("curl: (7)")
        || lower.contains("curl: (6)")
        || lower.contains("curl: (28)")
        || lower.contains("\"exit_code\":7")
        || lower.contains("\"reachable\":false")
}

fn extract_target_from_content(content: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(url) = v.get("url").and_then(|v| v.as_str()) {
            return Some(url.to_string());
        }
        // Also check nested result
        if let Some(inner) = v.get("result").and_then(|v| v.as_str()) {
            return extract_target_from_content(inner);
        }
    }
    None
}

fn target_host_from_url(url: &str) -> String {
    url.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .split(':')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
}
