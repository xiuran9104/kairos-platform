use std::sync::Arc;

use chrono::Utc;
use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
    ConversationTurnStatus,
};

use super::{
    ConversationAuthorizationService, ConversationContextSource, ConversationEventSink,
    ConversationInstructionSink, ConversationIntentDetector, ConversationKnowledgeSource,
    ConversationModelGateway, ConversationRepository, ModelTurnOutput,
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
        self.repository
            .ensure_execution_run(turn_id, &request.session_id)
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

        // ── Phase 0: Planning (like PentAGI CLI's should_prepare_tool_plan) ──
        let user_input = context
            .recent_messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let scan_goal = format!("{user_input}\n{}", context.objective_snapshot);
        let mut scan_progress = super::scan_progress::ScanProgress::new(&scan_goal);
        if needs_planning(&user_input) {
            let plan = self
                .generate_plan(turn_id, session_id, &user_input, &context, intent)
                .await?;
            // Write the plan as an assistant message so SSE shows it
            self.repository
                .write_intermediate_message(turn_id, session_id, "assistant", &plan, "text")
                .await?;
        }

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

            let output = match self.model_gateway.generate(&context, intent).await {
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
                    if request_count >= budget.max_requests {
                        stopped_by_budget = Some("requests");
                        break;
                    }
                    request_count += 1;
                    scan_progress.record_tool(&tc.name);
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
                    scan_progress.record_tool_result(&tc.name, result.success, duplicate_skipped);
                    if result.success && has_effective_observation(&tc.name, &result.content) {
                        successful_observations += 1;
                    }
                    let evidence_id = if let Some(run_id) = context.run_id.as_deref() {
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
                        Some(evidence_id)
                    } else {
                        None
                    };
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
                if stopped_by_budget.is_some() {
                    break;
                }
                context.run_summary = run_summary.clone();
                self.repository
                    .checkpoint_context(turn_id, &context, &run_summary, &evidence_ids)
                    .await?;

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

#[cfg(test)]
mod tests {
    use super::{
        MAX_MODEL_TOOL_OBSERVATION_CHARS, compact_model_observation, has_effective_observation,
        tool_request_fingerprint,
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
