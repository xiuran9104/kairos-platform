use std::sync::Arc;

use chrono::Utc;
use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
    ConversationTurnStatus,
};

use super::{
    ConversationAuthorizationService, ConversationContextSource, ConversationEventSink,
    ConversationInstructionSink, ConversationIntentDetector, ConversationModelGateway,
    ConversationRepository, ModelTurnOutput,
};

/// Maximum tool-calling rounds before forcing a stop.
/// PentAGI CLI uses 3 hours default runtime; the conversation agent
/// limits rounds instead. 50 rounds ≈ 1-2 hours of typical execution.
const MAX_TOOL_ROUNDS: usize = 50;
/// Absolute maximum runtime for a conversation turn (2 hours).
const MAX_TURN_RUNTIME_SECS: u64 = 2 * 60 * 60;

pub struct ConversationRuntime {
    repository: Arc<dyn ConversationRepository>,
    authorization: Arc<dyn ConversationAuthorizationService>,
    context_source: Arc<dyn ConversationContextSource>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    model_gateway: Arc<dyn ConversationModelGateway>,
    instruction_sink: Arc<dyn ConversationInstructionSink>,
    events: Arc<dyn ConversationEventSink>,
}

impl ConversationRuntime {
    pub fn new(
        repository: Arc<dyn ConversationRepository>,
        authorization: Arc<dyn ConversationAuthorizationService>,
        context_source: Arc<dyn ConversationContextSource>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
        model_gateway: Arc<dyn ConversationModelGateway>,
        instruction_sink: Arc<dyn ConversationInstructionSink>,
        events: Arc<dyn ConversationEventSink>,
    ) -> Self {
        Self {
            repository,
            authorization,
            context_source,
            intent_detector,
            model_gateway,
            instruction_sink,
            events,
        }
    }

    /// Create the turn and user message in the database, return (turn_id, run_id).
    /// The turn status is "thinking" — the caller is expected to call `execute_turn`
    /// in a background task.
    pub async fn begin_turn_only(
        &self,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<(String, Option<String>)> {
        let intent = self.intent_detector.detect(request, false).await?;
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
        let context = self.context_source.build(request).await?;
        let intent = self
            .intent_detector
            .detect(request, context.run_id.is_some())
            .await?;
        let run_id = context.run_id.clone();

        self.events
            .emit(
                "conversation.turn.started",
                serde_json::json!({"turn_id": turn_id, "session_id": request.session_id}),
            )
            .await?;

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
        let mut all_tool_results_text = String::new();
        let turn_start = std::time::Instant::now();

        // ── Phase 0: Planning (like PentAGI CLI's should_prepare_tool_plan) ──
        let user_input = context
            .recent_messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let mut scan_progress = super::scan_progress::ScanProgress::new(&user_input);
        if needs_planning(&user_input) {
            let plan = self
                .generate_plan(turn_id, session_id, &user_input, &context, intent)
                .await?;
            // Write the plan as an assistant message so SSE shows it
            self.repository
                .write_intermediate_message(turn_id, session_id, "assistant", &plan, "text")
                .await?;
            all_tool_results_text.push_str(&format!("## 📋 攻击计划\n\n{}\n\n---\n\n", plan));
        }

        for round in 0..MAX_TOOL_ROUNDS {
            if turn_start.elapsed().as_secs() >= MAX_TURN_RUNTIME_SECS {
                all_tool_results_text.push_str(&format!(
                    "\n\n⏱ 已达到最大运行时间限制 ({} 分钟)，停止执行。",
                    MAX_TURN_RUNTIME_SECS / 60
                ));
                break;
            }

            let output = match self.model_gateway.generate(&context, intent).await {
                Ok(output) => output,
                Err(error) => {
                    self.repository
                        .fail_turn(turn_id, "MODEL_UNAVAILABLE", &error.to_string(), true)
                        .await?;
                    return Err(anyhow::anyhow!("模型调用失败: {error}"));
                }
            };

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

                // Persist the plan/tool_calls message for SSE
                self.repository
                    .write_intermediate_message(
                        turn_id,
                        session_id,
                        "assistant",
                        &serde_json::json!({"tool_calls": tool_calls_json}).to_string(),
                        "tool_calls",
                    )
                    .await?;

                context.recent_messages.push(AgentMessage {
                    message_id: format!("assistant_tool:round_{}", round),
                    role: "assistant".into(),
                    content: serde_json::json!({"tool_calls": tool_calls_json}).to_string(),
                    format: "tool_calls".into(),
                });

                for tc in &output.tool_calls {
                    scan_progress.record_tool(&tc.name);
                    let result = super::tool_executor::execute_tool(
                        &tc.name,
                        &tc.arguments,
                        &context.scope_snapshot,
                    )
                    .await;
                    let result_text = format!(
                        "🔧 **{}** {}\n{}",
                        tc.name,
                        if result.success { "✅" } else { "❌" },
                        truncate_for_display(&result.content, 2000)
                    );
                    all_tool_results_text.push_str(&result_text);
                    all_tool_results_text.push('\n');

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
                            "result": result.content
                        })
                        .to_string(),
                        format: "tool_result".into(),
                    });
                }

                if !output.content.trim().is_empty() {
                    final_output = Some(output);
                }
                continue;
            }

            // A response without tool calls is only a candidate final response.
            // Scan objectives must first pass deterministic coverage gates.
            if let Some(prompt) = scan_progress.continuation_prompt(&output.content) {
                if !output.content.trim().is_empty() {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("assistant_gate:round_{round}"),
                        role: "assistant".into(),
                        content: output.content,
                        format: "text".into(),
                    });
                }
                context.recent_messages.push(AgentMessage {
                    message_id: format!("system_gate:round_{round}"),
                    role: "user".into(),
                    content: prompt,
                    format: "text".into(),
                });
                continue;
            }
            final_output = Some(output);
            break;
        }

        let output = final_output.ok_or_else(|| anyhow::anyhow!("模型未返回有效响应"))?;

        let final_content =
            if !all_tool_results_text.is_empty() && !output.content.trim().is_empty() {
                format!(
                    "{}\n\n---\n\n### 🔍 工具执行记录\n\n{}",
                    output.content, all_tool_results_text
                )
            } else if !output.content.trim().is_empty() {
                output.content.clone()
            } else {
                format!("### 🔍 工具执行结果\n\n{}", all_tool_results_text)
            };

        Ok(final_content)
    }
}

fn truncate_for_display(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        // Find a valid UTF-8 character boundary at or before max_chars
        let mut end = max_chars;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…(截断，共 {} 字符)", &text[..end], text.len())
    }
}

/// Check if the user input needs a plan before tool execution.
/// Mirrors PentAGI CLI's `should_prepare_tool_plan`.
fn needs_planning(input: &str) -> bool {
    let input = input.trim();
    input.len() >= 80
        || input.contains("http://")
        || input.contains("https://")
        || input.contains("渗透")
        || input.contains("漏洞")
        || input.contains("测试")
        || input.contains("分析")
        || input.contains("扫描")
        || input.contains("目标")
        || input.contains("评估")
        || input.contains("安全")
}

impl ConversationRuntime {
    /// Generate a plan before tool execution, similar to PentAGI CLI's tool_plan_instruction.
    async fn generate_plan(
        &self,
        turn_id: &str,
        session_id: &str,
        user_input: &str,
        context: &ConversationContext,
        intent: &ConversationIntent,
    ) -> anyhow::Result<String> {
        // Build a planning-specific context with only the system prompt + user input
        let mut plan_context = context.clone();
        // Add a planning instruction as a user message
        plan_context.recent_messages.push(AgentMessage {
            message_id: format!("plan_req:{}", turn_id),
            role: "user".into(),
            content: format!(
                "在调用任何工具之前，请先用中文输出一份简洁的攻击计划。\n\n\
                 用户任务：{user_input}\n\n\
                 计划要求：\n\
                 1. 列出你将使用的工具和顺序\n\
                 2. 说明每个阶段的目标\n\
                 3. 标注预期风险等级\n\
                 4. 不要调用工具，只输出计划文本\n\n\
                 输出格式：## 📋 攻击计划\\n\\n### 阶段1：信息收集\\n- ...\\n\\n### 阶段2：...\\n- ..."
            ),
            format: "text".into(),
        });

        let output = self.model_gateway.generate(&plan_context, intent).await?;
        let plan = if output.content.trim().is_empty() {
            "## 📋 攻击计划\n\n### 阶段1：信息收集\n- 探测目标 HTTP/HTTPS 服务存活状态\n- 爬取页面收集 JS 文件和 API 端点\n- DNS 查询获取域名信息\n\n### 阶段2：分析\n- 分析收集到的信息\n- 识别潜在攻击面\n\n### 阶段3：验证\n- 对发现的漏洞点进行验证".to_string()
        } else {
            output.content
        };
        Ok(plan)
    }
}
