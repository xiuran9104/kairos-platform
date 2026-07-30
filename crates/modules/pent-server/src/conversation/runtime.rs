use std::sync::Arc;

use chrono::Utc;
use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
    ConversationMessageType, ConversationTurnStatus,
};

use super::{
    ConversationAuthorizationService, ConversationContextSource, ConversationEventSink,
    ConversationInstructionSink, ConversationIntentDetector, ConversationKnowledgeSource,
    ConversationModelGateway, ConversationRepository, ModelTurnOutput,
};

/// Maximum tool-calling rounds before forcing a stop.
/// PentAGI CLI uses 3 hours default runtime; the conversation agent
/// limits rounds instead. 50 rounds ≈ 1-2 hours of typical execution.
const MAX_TOOL_ROUNDS: usize = 20;
/// Absolute maximum runtime for a conversation turn (2 hours).
const MAX_TURN_RUNTIME_SECS: u64 = 2 * 60 * 60;
/// Tool observations sent back to the model are compact. The complete response
/// has already been persisted as Evidence before this representation is used.
const MAX_MODEL_TOOL_OBSERVATION_CHARS: usize = 8_000;
const MAX_RUN_SUMMARY_CHARS: usize = 12_000;

/// Autonomous agent lifecycle actions. The model MUST emit one of these
/// (as a text prefix or JSON block) when it does not issue tool calls.
/// Ordinary chat without a security objective does not need these markers.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AgentDirective {
    /// Task complete; factual conclusions cite real Evidence IDs.
    Final(String),
    /// Blocked on missing information that only the user can provide.
    AskUser(String),
    /// No viable path within current authorization, tools, and budget.
    NoPath(String),
    /// Current strategy failed; switch approach and execute immediately.
    Replan(String),
    /// Text-only response with no explicit directive — treated as a
    /// candidate FINAL and subject to the evidence completion gate.
    CandidateFinal(String),
}

/// Autonomous loop injection message sent to the model at the start of every
/// tool loop. This reminds the model it is operating in autonomous mode and
/// must directly call tools or emit lifecycle actions.
const AUTONOMOUS_LOOP_PROMPT: &str = "\
## 自主工具循环已启动\n\n\
你现在处于自主执行模式。根据目标、当前证据和可用工具直接选择下一步操作：\n\
- 需要信息 → 直接发起 Tool Calling\n\
- 任务完成 → 输出 `FINAL: <结论>`（事实性结论必须引用 Evidence ID）\n\
- 缺少关键信息 → 输出 `ASK_USER: <问题>`\n\
- 无可行路径 → 输出 `NO_PATH: <原因>`\n\
- 策略失败需调整 → 输出 `REPLAN: <新策略>` 并在下一轮直接执行\n\n\
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
            .unwrap_or_else(|_| std::env::var("PENTA_ENV").as_deref() == Ok("development"))
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
}

fn budget_u64(value: &serde_json::Value, keys: &[&str], default: u64) -> u64 {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
        .unwrap_or(default)
}

pub struct ConversationRuntime {
    repository: Arc<dyn ConversationRepository>,
    authorization: Arc<dyn ConversationAuthorizationService>,
    context_source: Arc<dyn ConversationContextSource>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    model_gateway: Arc<dyn ConversationModelGateway>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    instruction_sink: Arc<dyn ConversationInstructionSink>,
    events: Arc<dyn ConversationEventSink>,
    nats: Option<crate::transport::mq::NatsHandle>,
}

impl ConversationRuntime {
    pub fn new(
        repository: Arc<dyn ConversationRepository>,
        authorization: Arc<dyn ConversationAuthorizationService>,
        context_source: Arc<dyn ConversationContextSource>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
        model_gateway: Arc<dyn ConversationModelGateway>,
        knowledge_source: Arc<dyn ConversationKnowledgeSource>,
        instruction_sink: Arc<dyn ConversationInstructionSink>,
        events: Arc<dyn ConversationEventSink>,
    ) -> Self {
        Self {
            repository,
            authorization,
            context_source,
            intent_detector,
            model_gateway,
            knowledge_source,
            instruction_sink,
            events,
            nats: None,
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
        self.authorization
            .reconcile_explicit_authorization(&request.session_id, &request.content)
            .await?;
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
                .record_run_event(run_id, "run.turn_started", &serde_json::json!({"turn_id": turn_id}))
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
        Ok(())
    }

    pub async fn handle(&self, request: &AgentTurnRequest) -> anyhow::Result<AgentTurnResult> {
        self.authorization
            .reconcile_explicit_authorization(&request.session_id, &request.content)
            .await?;
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
        let turn_start = std::time::Instant::now();
        let budget = TurnBudget::from_snapshot(&context.budget_snapshot);
        let mut request_count = 0usize;
        // Per-turn budget counts actual model API usage below. Historical
        // messages are windowed by the model gateway and are already included
        // in each provider-reported input_tokens value.
        let mut token_count = 0u64;
        let mut successful_observations = 0usize;
        let mut stopped_by_budget: Option<&'static str> = None;
        let mut rounds_used = 0usize;
        let mut finished_normally = false;
        let mut evidence_ids = Vec::new();
        let mut run_summary = context.run_summary.clone();
        let mut request_fingerprints = std::collections::HashSet::new();

        let user_input = context
            .recent_messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let scan_goal = format!("{user_input}\n{}", context.objective_snapshot);
        let mut scan_progress = super::scan_progress::ScanProgress::new(&scan_goal);
        if let Some(run_id) = context.run_id.as_deref() {
            let history = self.repository.load_run_tool_history(run_id).await?;
            evidence_ids = history.evidence_ids;
            for historical in history.tool_calls {
                request_fingerprints.insert(tool_request_fingerprint(
                    &historical.tool_name,
                    &historical.arguments.to_string(),
                ));
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

        for round in 0..budget.max_tool_rounds {
            rounds_used = round + 1;
            if turn_start.elapsed().as_secs() >= budget.max_duration_seconds {
                stopped_by_budget = Some("time");
                break;
            }
            if token_count >= budget.max_tokens {
                stopped_by_budget = Some("tokens");
                break;
            }

            let mut output = match self.model_gateway.generate(&context, intent).await {
                Ok(output) => output,
                Err(error) => {
                    self.repository
                        .fail_turn(turn_id, "MODEL_UNAVAILABLE", &error.to_string(), true)
                        .await?;
                    return Err(anyhow::anyhow!("模型调用失败: {error}"));
                }
            };
            token_count = token_count
                .saturating_add(output.usage.input_tokens)
                .saturating_add(output.usage.output_tokens);

            if !output.tool_calls.is_empty() {
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
                        message_id: format!("runtime_stop_{round}"), role: "user".into(),
                        content: "停止调用工具，输出最终报告。".into(), format: "text".into(),
                    });
                }

                let mut loop_diagnostic = None;

                // ── Phase 1: separate fast inline tools from slow parallel tools ──
                let mut fast_results: Vec<(usize, super::tool_executor::ConversationToolResult, String, String, std::time::Duration)> = Vec::new();
                let mut slow_spawns: Vec<(usize, String, String, String, tokio::task::JoinHandle<(String, String, String, super::tool_executor::ConversationToolResult, u64)>)> = Vec::new();

                for (i, tc) in output.tool_calls.iter().enumerate() {
                    if request_count >= budget.max_requests {
                        stopped_by_budget = Some("requests");
                        break;
                    }
                    request_count += 1;
                    let tool_started = std::time::Instant::now();
                    let fingerprint = tool_request_fingerprint(&tc.name, &tc.arguments);
                    if !request_fingerprints.insert(fingerprint) {
                        fast_results.push((i, super::tool_executor::ConversationToolResult {
                            tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                            content: serde_json::json!({"duplicate_skipped":true,"message":"同一工具请求已在本轮执行"}).to_string(),
                            success: true,
                        }, tc.id.clone(), tc.name.clone(), tool_started.elapsed()));
                    } else if matches!(tc.name.as_str(), "knowledge_search" | "evidence_list" | "evidence_search" | "evidence_view" | "traffic_list" | "traffic_view" | "traffic_sitemap" | "traffic_diff" | "traffic_replay" | "skill_list" | "skill_load") {
                        let result = self.execute_fast_tool(tc, session_id, &context).await;
                        fast_results.push((i, result, tc.id.clone(), tc.name.clone(), tool_started.elapsed()));
                    } else {
                        let call_id = tc.id.clone();
                        let name = tc.name.clone();
                        let args = tc.arguments.clone();
                        let scope = context.scope_snapshot.clone();
                        let sid = session_id.to_string();
                        let handle = tokio::spawn(async move {
                            let start = std::time::Instant::now();
                            let result = super::tool_executor::execute_tool(&name, &args, &scope, &sid).await;
                            (call_id, name, args, result, start.elapsed().as_millis() as u64)
                        });
                        slow_spawns.push((i, tc.id.clone(), tc.name.clone(), tc.arguments.clone(), handle));
                    }
                }

                // ── Phase 2: await all parallel tasks ──────────────────
                for (_idx, call_id, tool_name, tool_args, handle) in slow_spawns {
                    match handle.await {
                        Ok((cid, name, args, result, elapsed_ms)) => {
                            fast_results.push((0, result, cid, name, std::time::Duration::from_millis(elapsed_ms)));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, tool = %tool_name, "parallel tool panicked");
                            fast_results.push((0, super::tool_executor::ConversationToolResult {
                                tool_call_id: call_id.clone(), tool_name: tool_name.clone(),
                                content: serde_json::json!({"error": format!("{e}")}).to_string(), success: false,
                            }, call_id, tool_name, std::time::Duration::ZERO));
                        }
                    }
                }

                // ── Phase 3: process all results (SSE + evidence + traffic + summary) ──
                for (_idx, result, call_id, tool_name, elapsed) in &fast_results {
                    let tool_args = ""; // fast tools use empty args for persistence
                    // SSE: stream tool result as it arrives
                    let event_payload = serde_json::json!({
                        "turn_id": turn_id,
                        "tool_name": tool_name,
                        "tool_call_id": call_id,
                        "success": result.success,
                        "summary": result.content.chars().take(300).collect::<String>(),
                    });
                    self.events.emit("conversation.tool.completed", event_payload.clone()).await.ok();
                    // NATS: publish for external subscribers (frontend SSE, monitoring)
                    if let Some(ref nats) = self.nats {
                        let payload = serde_json::to_vec(&event_payload).unwrap_or_default();
                        nats.publish(&format!("penta.conversation.tools.{}", turn_id), &payload).await;
                    }

                    let duplicate_skipped = result.content.contains(r#""duplicate_skipped":true"#);
                    let effective_observation = result.success && !duplicate_skipped
                        && has_effective_observation(tool_name, &result.content);
                    if effective_observation { successful_observations += 1; }
                    if let Some(diagnostic) = scan_progress.record_tool_result(
                        tool_name, result.success, duplicate_skipped, effective_observation, &result.content,
                    ) { loop_diagnostic = Some(diagnostic); }
                    if tool_name == "traffic_replay" && result.success { scan_progress.record_traffic_replay(); }

                    let evidence_id = if is_evidence_tool(tool_name) || is_traffic_tool(tool_name) {
                        None
                    } else if let Some(run_id) = context.run_id.as_deref() {
                        let eid = self.repository.record_tool_result(
                            run_id, call_id, tool_name, &tool_args, result.success, &result.content,
                            elapsed.as_millis().try_into().unwrap_or(u64::MAX),
                        ).await?;
                        evidence_ids.push(eid.clone());
                        scan_progress.record_evidence_tool(&eid, tool_name);
                        Some(eid)
                    } else { None };

                    if let Some(run_id) = context.run_id.as_deref()
                        && matches!(tool_name.as_str(), "http_request" | "http_probe")
                    {
                        self.repository.record_http_traffic(run_id, call_id, tool_name, &tool_args, &result.content, None).await?;
                    }
                    append_run_summary(&mut run_summary, round + 1, tool_name, result.success, evidence_id.as_deref(), &result.content);

                    let display = serde_json::json!({"tool_name":tool_name,"tool_call_id":call_id,"success":result.success,"result":result.content});
                    self.repository.write_intermediate_message(turn_id, session_id, "tool", &display.to_string(), "tool_result").await?;
                    // Also write to pent_run_events so Run detail page shows progress
                    if let Some(ref run_id) = context.run_id {
                        self.repository.record_run_event(run_id, "tool.executed", &display).await?;
                    }
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("tool_result:{}", call_id), role: "tool".into(),
                        content: serde_json::json!({"tool_call_id":call_id,"result":compact_model_observation(&result.content)}).to_string(),
                        format: "tool_result".into(),
                    });
                }

                // End of parallel tool execution (original loop ended here)
                #[allow(unreachable_code)]
                if false {
                for tc in &output.tool_calls {
                    if request_count >= budget.max_requests {
                        stopped_by_budget = Some("requests");
                        break;
                    }
                    request_count += 1;
                    let tool_started = std::time::Instant::now();
                    let fingerprint = tool_request_fingerprint(&tc.name, &tc.arguments);
                    let result = if !request_fingerprints.insert(fingerprint) {
                        super::tool_executor::ConversationToolResult {
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            content: serde_json::json!({
                                "duplicate_skipped": true,
                                "message": "同一工具请求已在本轮执行，使用已有观察并推进下一阶段"
                            })
                            .to_string(),
                            success: true,
                        }
                    } else if tc.name == "knowledge_search" {
                        self.execute_knowledge_search(&tc.id, &tc.arguments).await
                    } else if is_skill_tool(&tc.name) {
                        let content = self
                            .repository
                            .execute_skill_tool(&tc.name, &tc.arguments)
                            .await
                            .map(|value| value.to_string());
                        super::tool_executor::ConversationToolResult {
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            success: content.is_ok(),
                            content: content.unwrap_or_else(|error| {
                                serde_json::json!({
                                    "error": error.to_string(),
                                    "retryable": false
                                })
                                .to_string()
                            }),
                        }
                    } else if is_traffic_tool(&tc.name) {
                        let content = match context.run_id.as_deref() {
                            Some(run_id) => {
                                self.execute_traffic_tool(
                                    run_id,
                                    session_id,
                                    &tc.name,
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
                            content: content.unwrap_or_else(|error| {
                                serde_json::json!({
                                    "error": error.to_string(),
                                    "retryable": false
                                })
                                .to_string()
                            }),
                        }
                    } else if is_evidence_tool(&tc.name) {
                        let content = match context.run_id.as_deref() {
                            Some(run_id) => self
                                .repository
                                .execute_evidence_tool(run_id, &tc.name, &tc.arguments)
                                .await
                                .map(|value| value.to_string()),
                            None => Err(anyhow::anyhow!("Evidence 工具只能在已有执行 Run 中使用")),
                        };
                        super::tool_executor::ConversationToolResult {
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            success: content.is_ok(),
                            content: content.unwrap_or_else(|error| {
                                serde_json::json!({
                                    "error": error.to_string(),
                                    "retryable": false
                                })
                                .to_string()
                            }),
                        }
                    } else {
                        super::tool_executor::execute_tool(
                            &tc.name,
                            &tc.arguments,
                            &context.scope_snapshot,
                            session_id,
                        )
                        .await
                    };
                    let duplicate_skipped = result.content.contains(r#""duplicate_skipped":true"#);
                    let effective_observation = result.success
                        && !duplicate_skipped
                        && has_effective_observation(&tc.name, &result.content);
                    if effective_observation {
                        successful_observations += 1;
                    }
                    if let Some(diagnostic) = scan_progress.record_tool_result(
                        &tc.name,
                        result.success,
                        duplicate_skipped,
                        effective_observation,
                        &result.content,
                    ) {
                        loop_diagnostic = Some(diagnostic);
                    }
                    if tc.name == "traffic_replay" && result.success {
                        scan_progress.record_traffic_replay();
                    }
                    let evidence_id = if is_evidence_tool(&tc.name) || is_traffic_tool(&tc.name) {
                        // Reading Evidence/Traffic must not create Evidence-of-Evidence
                        // records or pollute target-observation completion gates.
                        None
                    } else if let Some(run_id) = context.run_id.as_deref() {
                        let evidence_id = self
                            .repository
                            .record_tool_result(
                                run_id,
                                &tc.id,
                                &tc.name,
                                &tc.arguments,
                                result.success,
                                &result.content,
                                tool_started
                                    .elapsed()
                                    .as_millis()
                                    .try_into()
                                    .unwrap_or(u64::MAX),
                            )
                            .await?;
                        evidence_ids.push(evidence_id.clone());
                        scan_progress.record_evidence_tool(&evidence_id, &tc.name);
                        Some(evidence_id)
                    } else {
                        None
                    };
                    if let Some(run_id) = context.run_id.as_deref()
                        && matches!(tc.name.as_str(), "http_request" | "http_probe")
                    {
                        self.repository
                            .record_http_traffic(
                                run_id,
                                &tc.id,
                                &tc.name,
                                &tc.arguments,
                                &result.content,
                                None,
                            )
                            .await?;
                    }
                    append_run_summary(
                        &mut run_summary,
                        round + 1,
                        &tc.name,
                        result.success,
                        evidence_id.as_deref(),
                        &result.content,
                    );
                    // Persist tool result for SSE
                    let display_content = serde_json::json!({
                        "tool_name": tc.name,
                        "tool_call_id": tc.id,
                        "success": result.success,
                        "result": result.content
                    });
                    self.repository
                        .write_intermediate_message(
                            turn_id,
                            session_id,
                            "tool",
                            &display_content.to_string(),
                            "tool_result",
                        )
                        .await?;

                    context.recent_messages.push(AgentMessage {
                        message_id: format!("tool_result:{}", tc.id),
                        role: "tool".into(),
                        content: serde_json::json!({
                            "tool_call_id": tc.id,
                            "result": compact_model_observation(&result.content)
                        })
                        .to_string(),
                        format: "tool_result".into(),
                    });
                }
                } // end if false (dead code block containing old serial loop)
                if stopped_by_budget.is_some() {
                    break;
                }
                context.run_summary = run_summary.clone();
                self.repository
                    .checkpoint_context(turn_id, &context, &run_summary, &evidence_ids)
                    .await?;

                if scan_progress.stall_exhausted() {
                    anyhow::bail!(
                        "RUN_BLOCKED: autonomous loop stalled after repeated calls without new evidence"
                    );
                }
                if let Some(diagnostic) = loop_diagnostic {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_diagnostic:round_{round}"),
                        role: "user".into(),
                        content: diagnostic,
                        format: "text".into(),
                    });
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
                    output.content = nonempty_or(
                        &question,
                        "需要用户补充继续执行所必需的目标、授权范围、测试凭据或业务背景。",
                    )
                    .to_string();
                }
                AgentDirective::Final(candidate) | AgentDirective::CandidateFinal(candidate) => {
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
                }
            }
            final_output = Some(output);
            finished_normally = true;
            break;
        }

        if let Some(kind) = stopped_by_budget {
            anyhow::bail!(
                "RUN_INCOMPLETE: {} budget exhausted (rounds={}, requests={}, tokens={})",
                kind,
                rounds_used,
                request_count,
                token_count
            );
        }
        if !finished_normally {
            anyhow::bail!(
                "RUN_INCOMPLETE: max_tool_rounds exhausted ({})",
                budget.max_tool_rounds
            );
        }
        if request_count > 0 && successful_observations == 0 {
            anyhow::bail!(
                "RUN_BLOCKED: all tool calls failed or returned no effective observation"
            );
        }
        let output = final_output.ok_or_else(|| anyhow::anyhow!("模型未返回有效响应"))?;

        let final_content = if context.run_id.is_some() {
            let report = if output.content.trim().is_empty() {
                "本轮未生成文字结论；请依据 Evidence 进行复核。"
            } else {
                output.content.trim()
            };
            let run_reference = context.run_id.as_deref().unwrap_or("无");
            format!(
                "{report}\n\n---\n\n## 结构化证据索引\n\n\
                 - 工具调用：{request_count}\n- 有效观察：{successful_observations}\n\
                 - Evidence 数量：{}\n- Run：`{run_reference}`\n\n\
                 各项发现应引用其 Evidence；完整证据可在该 Run 的 Evidence 页面复核。\
                 原始响应未直接拼接进报告。",
                evidence_ids.len()
            )
        } else if !output.content.trim().is_empty() {
            output.content.clone()
        } else {
            format!(
                "本轮完成了 {request_count} 次受控工具调用并形成 {successful_observations} 条有效观察。\
                 原始响应已保存为 Evidence，报告不内联原始输出。"
            )
        };

        Ok(final_content)
    }

    /// Execute a fast inline tool (knowledge, skill, traffic, evidence) synchronously.
    async fn execute_fast_tool(
        &self,
        tc: &super::ports::ModelToolCall,
        session_id: &str,
        context: &ConversationContext,
    ) -> super::tool_executor::ConversationToolResult {
        match tc.name.as_str() {
            "knowledge_search" => self.execute_knowledge_search(&tc.id, &tc.arguments).await,
            "skill_list" | "skill_load" => {
                let content = self.repository.execute_skill_tool(&tc.name, &tc.arguments).await.map(|v| v.to_string());
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()),
                }
            }
            name if is_traffic_tool(name) => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => self.execute_traffic_tool(run_id, session_id, name, &tc.id, &tc.arguments, &context.scope_snapshot).await,
                    None => Err(anyhow::anyhow!("流量工具只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()),
                }
            }
            name if is_evidence_tool(name) => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => self.repository.execute_evidence_tool(run_id, name, &tc.arguments).await.map(|v| v.to_string()),
                    None => Err(anyhow::anyhow!("Evidence 工具只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()),
                }
            }
            _ => super::tool_executor::execute_tool(&tc.name, &tc.arguments, &context.scope_snapshot, session_id).await,
        }
    }

    /// Emit event to SSE sink and NATS (if connected).
    async fn emit_event(&self, event: &str, payload: serde_json::Value) {
        self.events.emit(event, payload.clone()).await.ok();
        if let Some(ref nats) = self.nats {
            let bytes = serde_json::to_vec(&payload).unwrap_or_default();
            nats.publish(&format!("penta.conversation.{}", event), &bytes).await;
        }
    }

    async fn execute_knowledge_search(
        &self,
        tool_call_id: &str,
        arguments: &str,
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
        match self.knowledge_source.search_guides(query, 4).await {
            Ok(guides) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "knowledge_search".into(),
                content: serde_json::json!({
                    "query": query,
                    "guide_count": guides.len(),
                    "guides": guides,
                    "instruction": "根据 Wiki 方法选择当前可用工具继续验证；不要仅复述知识。"
                })
                .to_string(),
                success: !guides.is_empty(),
            },
            Err(error) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "knowledge_search".into(),
                content: serde_json::json!({"error":error.to_string()}).to_string(),
                success: false,
            },
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
        let result = super::tool_executor::execute_tool(
            "http_request",
            &replay_args.to_string(),
            scope_snapshot,
            session_id,
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
            object.insert(
                "replay_traffic_ids".into(),
                serde_json::json!(traffic_ids),
            );
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

fn tool_request_fingerprint(tool_name: &str, arguments: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return format!("{tool_name}:{arguments}");
    };
    if tool_name == "http_request" {
        return serde_json::json!({
            "tool": tool_name,
            "method": value.get("method").and_then(serde_json::Value::as_str)
                .unwrap_or("GET").to_ascii_uppercase(),
            "url": value.get("url").and_then(serde_json::Value::as_str)
                .unwrap_or("").trim_end_matches('/'),
            "body": value.get("body").cloned().unwrap_or(serde_json::Value::Null),
            "form": value.get("form").cloned().unwrap_or(serde_json::Value::Null),
        })
        .to_string();
    }
    format!("{tool_name}:{value}")
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

/// Parse the model's text-only response into an autonomous agent directive.
///
/// Supports two formats:
/// 1. **Text prefix** — `FINAL: <content>`, `ASK_USER: <question>`, etc.
///    Chinese equivalents like `最终结果：` and `需要用户：` are also accepted.
/// 2. **JSON action block** — A JSON object with an `action` field:
///    `{"action":"final","content":"...","evidence_refs":["ev-1"]}`
///    This format is preferred when the model can produce structured output.
fn parse_agent_directive(content: &str) -> AgentDirective {
    let content = content.trim();

    // ── JSON structured action protocol ──────────────────────────
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
            match action {
                "final" | "FINAL" => return AgentDirective::Final(fallback),
                "ask_user" | "ASK_USER" => return AgentDirective::AskUser(fallback),
                "no_path" | "NO_PATH" => return AgentDirective::NoPath(fallback),
                "replan" | "REPLAN" => return AgentDirective::Replan(fallback),
                _ => {}
            }
        }
    }

    // ── Text prefix protocol ─────────────────────────────────────
    for marker in ["FINAL:", "Final:", "final:", "最终结果：", "完成："] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::Final(value.trim().to_string());
        }
    }
    for marker in [
        "ASK_USER:",
        "Ask user:",
        "ask_user:",
        "需要用户：",
        "请用户确认：",
    ] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::AskUser(value.trim().to_string());
        }
    }
    for marker in [
        "NO_PATH:",
        "No viable path:",
        "no_path:",
        "无法继续：",
        "没有可继续验证的路径：",
    ] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::NoPath(value.trim().to_string());
        }
    }
    for marker in ["REPLAN:", "Replan:", "replan:", "重新规划："] {
        if let Some(value) = content.strip_prefix(marker) {
            return AgentDirective::Replan(value.trim().to_string());
        }
    }
    AgentDirective::CandidateFinal(content.to_string())
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

fn nonempty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentDirective, MAX_MODEL_TOOL_OBSERVATION_CHARS, compact_model_observation,
        has_effective_observation, parse_agent_directive, tool_request_fingerprint,
    };

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
    fn http_request_fingerprint_ignores_rotating_cookie() {
        let first = tool_request_fingerprint(
            "http_request",
            r#"{"url":"http://target.test/a","headers":{"Cookie":"session=one"}}"#,
        );
        let second = tool_request_fingerprint(
            "http_request",
            r#"{"headers":{"Cookie":"session=two"},"url":"http://target.test/a/"}"#,
        );
        assert_eq!(first, second);
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
            AgentDirective::CandidateFinal("目标似乎没有开放端口".into())
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

fn is_skill_tool(tool_name: &str) -> bool {
    matches!(tool_name, "skill_list" | "skill_load")
}

fn is_reference_tool(tool_name: &str) -> bool {
    tool_name == "knowledge_search" || is_evidence_tool(tool_name) || is_skill_tool(tool_name)
}
