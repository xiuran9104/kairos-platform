//! LLM-03: ModelGateway using AiModelFactory (ai-server).
use crate::conversation::ports::{ConversationModelGateway, ModelToolCall, ModelTurnOutput};
use async_trait::async_trait;
use kairos_platform_ai_api::{ChatMessage, ChatRequest};
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{ConversationContext, ConversationIntent, ModelUsage};
use kairos_platform_pent_core::conversation::DEFAULT_CONTEXT_TOKEN_BUDGET;
use serde_json::Value;

use std::collections::HashSet;
use std::sync::Mutex;

pub struct AiModelGateway {
    pool: PgPool,
    factory: AiModelFactory,
    exhausted_tools: Mutex<HashSet<String>>,
}

impl AiModelGateway {
    pub fn new(pool: PgPool) -> Self {
        Self {
            factory: AiModelFactory::new(pool.clone()),
            pool,
            exhausted_tools: Mutex::new(HashSet::new()),
        }
    }

    pub(crate) async fn resolve_model(
        &self,
        session_id: &str,
    ) -> anyhow::Result<(i64, Option<f64>, u32)> {
        let selected: Option<i64> = sqlx::query_scalar(
            "SELECT m.id
             FROM pent_agent_sessions s
             JOIN ai.model_configs m
               ON m.id=NULLIF(s.constraints->>'model_id','')::bigint
              AND m.type='chat' AND m.status=0
             WHERE s.id=$1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pent_agent_sessions WHERE id=$1)",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        if !exists {
            return Err(anyhow::anyhow!("Agent Session 不存在"));
        }
        let id = if let Some(id) = selected {
            id
        } else {
            sqlx::query_scalar(
            "SELECT id FROM ai.model_configs
             WHERE type = 'chat' AND status = 0
             ORDER BY id LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("未配置可用的聊天模型"))?
        };
        Ok((id, Some(0.3), conversation_output_token_limit(2000)))
    }
}

fn conversation_output_token_limit(configured: u32) -> u32 {
    if std::env::var("PENTA_ENV").as_deref() == Ok("development") {
        return std::env::var("PENTA_CONVERSATION_MAX_OUTPUT_TOKENS")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(16_000)
            .clamp(1, 16_000);
    }
    configured
}

/// Build the routing intent from the run objective, rolling summary and the
/// most recent user message so tool routing reflects the current phase.
fn routing_intent(context: &ConversationContext) -> String {
    let last_user = context
        .recent_messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.clone())
        .unwrap_or_default();
    format!(
        "{}\n{}\n{}",
        context.objective_snapshot, context.run_summary, last_user
    )
}

/// Max risk accepted for routed tools. Conversation-executable tools are
/// already allow-listed to low/medium risk; the policy snapshot can tighten
/// this further via `max_risk`.
fn routing_max_risk(context: &ConversationContext) -> &'static str {
    match context
        .policy_snapshot
        .get("max_risk")
        .and_then(Value::as_str)
    {
        Some("info") | Some("low") => "low",
        Some("medium") => "medium",
        Some("critical") => "critical",
        _ => "high",
    }
}

#[async_trait]
impl ConversationModelGateway for AiModelGateway {
    fn set_exhausted_tools(&self, tools: HashSet<String>) {
        if let Ok(mut guard) = self.exhausted_tools.lock() {
            *guard = tools;
        }
    }

    async fn generate(
        &self,
        context: &ConversationContext,
        intent: &ConversationIntent,
    ) -> anyhow::Result<ModelTurnOutput> {
        let (model_id, temperature, max_tokens) = self.resolve_model(&context.session_id).await?;
        let context_token_budget = std::env::var("PENTA_CONTEXT_TOKEN_BUDGET")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| context.policy_snapshot.pointer("/multi_agent/context_budget_tokens").and_then(Value::as_u64).unwrap_or(DEFAULT_CONTEXT_TOKEN_BUDGET))
            .clamp(2_000, 128_000);
        let model_context = context;
        let memory_context = if model_context.run_summary.trim().is_empty() {
            String::new()
        } else {
            format!("\n\n## 运行摘要与长期记忆\n{}", model_context.run_summary)
        };
        let intent_context = format!(
            "\n\n## 当前轮意图\n- 类型：{:?}\n- 置信度：{:.2}\n\
             意图只影响本轮交互方式，不得覆盖 Scope、策略、审批和预算。",
            intent.kind, intent.confidence
        );
        let mut messages = vec![ChatMessage {
            role: "system".into(),
            content: format!(
                "{}{}{}",
                model_context.system_instruction, memory_context, intent_context
            ),
        }];
        messages.extend(
            model_context
                .recent_messages
                .iter()
                .map(|message| ChatMessage {
                    role: message.role.clone(),
                    content: message.content.clone(),
                }),
        );
        let started = std::time::Instant::now();

        let finalizing_budget = context
            .recent_messages
            .last()
            .is_some_and(|message| message.message_id == "runtime_budget_finalization");
        let tools = if finalizing_budget {
            Vec::new()
        } else {
            // Full catalog: all active tools, no routing truncation.
            // The LLM is best positioned to select tools for each task.
            super::tool_catalog::load_tools_for_intent(
                &self.pool,
                &routing_intent(context),
                context.policy_snapshot.get("multi_agent").unwrap_or(&Value::Null),
            ).await?
        };
        // Filter out tools that have exceeded their consecutive failure limit.
        let tools: Vec<Value> = {
            let exhausted = self
                .exhausted_tools
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let sub_agent = context.policy_snapshot.get("_agent_role").and_then(Value::as_str) == Some("sub");
            tools
                    .into_iter()
                    .filter(|t| {
                        let name = t
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        !exhausted.contains(name)
                            && !(sub_agent && matches!(name, "agent_spawn" | "plan_create" | "plan_get" | "plan_update"))
                    })
                    .collect()
        };
        let mut api_messages: Vec<Value> =
            messages.iter().map(|m| format_message_for_api(m)).collect();

        // Context trimming or an interrupted turn can leave either side of the
        // tool protocol without its pair. Validate every group by tool_call_id.
        sanitize_tool_message_groups(&mut api_messages);
        if let Some(compaction) = super::context_compaction::prepare(&api_messages, context_token_budget) {
            let fallback = super::context_compaction::fallback_summary(&compaction.older);
            let transcript = serde_json::to_string(&compaction.older).unwrap_or_default();
            let summary = self.factory.chat(model_id, ChatRequest {
                model: String::new(),
                messages: vec![
                    ChatMessage { role: "system".into(), content: "压缩 Agent 历史上下文。必须保留目标、授权范围、已执行动作、Evidence ID、发现、失败原因、未完成步骤和用户约束；不得虚构。只输出摘要。".into() },
                    ChatMessage { role: "user".into(), content: transcript.chars().take(160_000).collect() },
                ],
                temperature: Some(0.0), max_tokens: Some(4000),
            }).await.ok().map(|response| response.content).filter(|v| !v.trim().is_empty()).unwrap_or(fallback);
            api_messages = super::context_compaction::merge(summary, compaction);
            sanitize_tool_message_groups(&mut api_messages);
        }
        trim_api_messages_preserving_tool_groups(&mut api_messages, context_token_budget);
        let response = self
            .factory
            .chat_tools(model_id, api_messages, tools, temperature, Some(max_tokens))
            .await
            .map_err(|error| anyhow::anyhow!("模型调用失败: {error:?}"))?;

        // Parse the response — it may contain either text or tool calls
        let (content, tool_calls) = parse_chat_tools_response(&response);

        if content.trim().is_empty() && tool_calls.is_empty() {
            tracing::warn!(
                model_id,
                finish_reason = response
                    .pointer("/choices/0/finish_reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("missing"),
                choices = response
                    .get("choices")
                    .and_then(|value| value.as_array())
                    .map_or(0, Vec::len),
                has_reasoning = response
                    .pointer("/choices/0/message/reasoning_content")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.trim().is_empty()),
                "model returned no content or valid tool calls"
            );
        }
        let input_tokens = response
            .get("usage")
            .and_then(|u| u.get("prompt_tokens"))
            .or_else(|| response.get("usage").and_then(|u| u.get("input_tokens")))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let output_tokens = response
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .or_else(|| response.get("usage").and_then(|u| u.get("output_tokens")))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(ModelTurnOutput {
            content,
            reasoning_summary: None,
            plan_update: None,
            citations: vec![],
            usage: ModelUsage {
                model_id: Some(model_id.to_string()),
                model_role: "conversation".into(),
                input_tokens,
                output_tokens,
                latency_ms: started.elapsed().as_millis() as u64,
            },
            tool_calls,
        })
    }
}

/// Format a message for the OpenAI-compatible chat/tools API.
/// Detects the message type from its content structure:
/// 1. Tool calls messages (content is JSON with "tool_calls" key) → `{role: "assistant", tool_calls: [...], content: null}`
/// 2. Tool result messages (content is JSON with "tool_call_id" key) → `{role: "tool", tool_call_id, content}`
/// 3. Regular messages → `{role, content}`
fn format_message_for_api(m: &kairos_platform_ai_api::ChatMessage) -> Value {
    // Try to parse content as JSON to detect tool-related messages
    if let Ok(parsed) = serde_json::from_str::<Value>(&m.content) {
        // Tool calls message from assistant
        if let Some(tool_calls) = parsed.get("tool_calls") {
            return serde_json::json!({
                "role": "assistant",
                "content": parsed.get("content").cloned().unwrap_or(Value::Null),
                "tool_calls": tool_calls
            });
        }
        // Tool result message
        if let Some(tool_call_id) = parsed.get("tool_call_id").and_then(Value::as_str) {
            let result_content = parsed
                .get("result")
                .map(|v| v.to_string())
                .unwrap_or_else(|| m.content.clone());
            return serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": result_content
            });
        }
    }
    // Regular message: just role + content
    serde_json::json!({"role": m.role, "content": m.content})
}

/// Keep complete `assistant.tool_calls` + `tool` response groups.
///
/// The runtime may insert RMOS injections, diagnostics, and other system/user
/// messages between an assistant tool_calls message and its tool results.
/// This function skips up to `MAX_INTERLEAVED_MESSAGES` non-tool messages
/// when looking for the matching tool results before giving up.
///
/// Truly orphan tool results (no preceding assistant *anywhere* in the
/// retained window) are still removed.
fn sanitize_tool_message_groups(api_messages: &mut Vec<Value>) {
    /// Maximum number of interleaved non-tool messages allowed between an
    /// assistant tool_calls and its tool results before the group is
    /// considered split.
    const MAX_INTERLEAVED_MESSAGES: usize = 5;

    let source = std::mem::take(api_messages);
    let mut sanitized = Vec::with_capacity(source.len());
    let mut index = 0;

    // Collect the set of all tool_call_ids that appear in assistant tool_calls
    // messages so we can tell which tool results are truly orphan.
    let all_expected_ids: std::collections::HashSet<String> = source
        .iter()
        .filter_map(|m| {
            if m.get("role").and_then(Value::as_str) != Some("assistant") {
                return None;
            }
            m.get("tool_calls")?.as_array()
        })
        .flatten()
        .filter_map(|call| call.get("id").and_then(Value::as_str).map(String::from))
        .collect();

    while index < source.len() {
        let message = &source[index];
        let role = message.get("role").and_then(Value::as_str);
        let tool_calls = message.get("tool_calls").and_then(Value::as_array);

        if role == Some("assistant") && tool_calls.is_some() {
            let expected: std::collections::HashSet<&str> = tool_calls
                .into_iter()
                .flatten()
                .filter_map(|call| call.get("id").and_then(Value::as_str))
                .collect();
            // Scan forward to collect matching tool results. Up to
            // MAX_INTERLEAVED_MESSAGES non-tool messages are tolerated
            // BEFORE the first tool result (RMOS injections, diagnostics).
            // Once we've seen a tool result, the very next non-tool message
            // terminates the group.
            let mut group_end = index + 1;
            let mut skipped = 0usize;
            let mut tool_indices: Vec<usize> = Vec::new();
            let mut interleaved_before: Vec<usize> = Vec::new();
            let mut found_first_tool = false;
            while group_end < source.len() {
                let next_role = source[group_end].get("role").and_then(Value::as_str);
                if next_role == Some("tool") {
                    tool_indices.push(group_end);
                    group_end += 1;
                    skipped = 0;
                    found_first_tool = true;
                } else if !found_first_tool
                    && skipped < MAX_INTERLEAVED_MESSAGES
                    && next_role.is_some_and(|r| r == "system" || r == "user")
                {
                    interleaved_before.push(group_end);
                    skipped += 1;
                    group_end += 1;
                } else {
                    break;
                }
            }
            // Collect actual tool_call_ids from the tool messages we found.
            let actual: Vec<&str> = tool_indices
                .iter()
                .filter_map(|&i| {
                    source[i]
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                })
                .collect();
            let actual_set: std::collections::HashSet<&str> = actual.iter().copied().collect();
            let complete =
                !expected.is_empty() && actual.len() == expected.len() && actual_set == expected;

            // Emit assistant → matching tool results → then interleaved
            // non-tool messages. This keeps assistant + tools consecutive so
            // trim_api_messages_preserving_tool_groups treats them as one
            // indivisible unit, preventing orphan tool results after context
            // trimming. The reordering is semantic-neutral because the model
            // reads the full context before generating.
            sanitized.push(source[index].clone());
            // Matching tool results first (keeps them consecutive with
            // the assistant).
            for &ti in &tool_indices {
                if complete || {
                    let id = source[ti]
                        .get("tool_call_id")
                        .and_then(Value::as_str);
                    id.is_some_and(|id| expected.contains(id))
                } {
                    sanitized.push(source[ti].clone());
                }
            }
            // Then interleaved non-tool messages that appeared before the
            // first tool result (RMOS injections, diagnostics, etc.).
            for &bi in &interleaved_before {
                sanitized.push(source[bi].clone());
            }
            if expected.is_empty() {
                tracing::debug!("assistant tool_calls with no call ids — kept as-is");
            } else if actual_set != expected {
                tracing::warn!(
                    expected = expected.len(),
                    actual = actual.len(),
                    interleaved = group_end - index - 1 - tool_indices.len(),
                    "incomplete tool message group — kept assistant + matching results"
                );
            }
            index = group_end;
            continue;
        }

        if role == Some("tool") {
            let tool_call_id = message.get("tool_call_id").and_then(Value::as_str);
            let is_orphan = tool_call_id.is_none_or(|id| !all_expected_ids.contains(id));
            if is_orphan {
                tracing::debug!(
                    tool_call_id = tool_call_id.unwrap_or("<missing>"),
                    "removed orphan tool result from model context"
                );
                index += 1;
                continue;
            }
            // Tool result whose parent assistant was trimmed away — keep it
            // as it pairs with a known assistant elsewhere in context.
            sanitized.push(message.clone());
            index += 1;
            continue;
        }

        sanitized.push(message.clone());
        index += 1;
    }

    *api_messages = sanitized;
}

/// Trim complete protocol units from the oldest side. An assistant tool-call
/// message and all consecutive tool results form one indivisible unit.
fn trim_api_messages_preserving_tool_groups(api_messages: &mut Vec<Value>, token_budget: u64) {
    if api_messages.is_empty() {
        return;
    }
    let source = std::mem::take(api_messages);
    let system_end = source
        .iter()
        .take_while(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .count();
    let mut units: Vec<std::ops::Range<usize>> = Vec::new();
    let mut index = system_end;
    while index < source.len() {
        let start = index;
        if source[index].get("role").and_then(Value::as_str) == Some("assistant")
            && source[index].get("tool_calls").is_some()
        {
            index += 1;
            while index < source.len()
                && source[index].get("role").and_then(Value::as_str) == Some("tool")
            {
                index += 1;
            }
        } else {
            index += 1;
        }
        units.push(start..index);
    }

    let fixed_tokens = source[..system_end]
        .iter()
        .map(estimate_api_message_tokens)
        .sum::<u64>();
    let available = token_budget.saturating_sub(fixed_tokens);
    let mut selected_start = units.len();
    let mut used = 0u64;
    for (unit_index, unit) in units.iter().enumerate().rev() {
        let cost = source[unit.clone()]
            .iter()
            .map(estimate_api_message_tokens)
            .sum::<u64>();
        if used.saturating_add(cost) > available && selected_start < units.len() {
            break;
        }
        used = used.saturating_add(cost);
        selected_start = unit_index;
    }

    api_messages.extend(source[..system_end].iter().cloned());
    if selected_start < units.len() {
        api_messages.extend(source[units[selected_start].start..].iter().cloned());
    }
}

fn estimate_api_message_tokens(message: &Value) -> u64 {
    message.to_string().chars().count().div_ceil(3) as u64
}

fn parse_chat_tools_response(response: &Value) -> (String, Vec<ModelToolCall>) {
    let choice = response.pointer("/choices/0/message").or_else(|| {
        response
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
    });

    let Some(message) = choice else {
        // Try direct text extraction
        let text = response
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return (text, vec![]);
    };

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let tool_calls: Vec<ModelToolCall> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let func = tc.get("function")?;
                    Some(ModelToolCall {
                        id: tc
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        name: func.get("name").and_then(Value::as_str)?.to_string(),
                        arguments: func
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    (content, tool_calls)
}

#[cfg(test)]
mod tests {
    use super::{
        format_message_for_api, sanitize_tool_message_groups,
        trim_api_messages_preserving_tool_groups,
    };
    use kairos_platform_ai_api::ChatMessage;
    use serde_json::{Value, json};

    fn call(id: &str) -> Value {
        json!({
            "role": "assistant",
            "tool_calls": [{"id": id, "type": "function", "function": {
                "name": "http_probe", "arguments": "{}"
            }}]
        })
    }

    fn result(id: &str) -> Value {
        json!({"role": "tool", "tool_call_id": id, "content": "{}"})
    }

    #[test]
    fn preserves_assistant_content_that_accompanies_tool_calls() {
        let message = ChatMessage {
            role: "assistant".into(),
            content: json!({
                "content": "根据新证据改为检查登录接口",
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": {"name": "http_request", "arguments": "{}"}
                }]
            })
            .to_string(),
        };
        let formatted = format_message_for_api(&message);
        assert_eq!(formatted["content"], "根据新证据改为检查登录接口");
        assert_eq!(formatted["tool_calls"][0]["id"], "call-1");
    }

    #[test]
    fn removes_orphan_tool_result_after_regular_message() {
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "u"}),
            result("missing"),
            json!({"role": "user", "content": "latest"}),
        ];
        sanitize_tool_message_groups(&mut messages);
        assert_eq!(messages.len(), 3);
        assert!(messages.iter().all(|m| m["role"] != "tool"));
    }

    #[test]
    fn preserves_complete_tool_group() {
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            call("call-1"),
            result("call-1"),
        ];
        sanitize_tool_message_groups(&mut messages);
        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn removes_mismatched_or_incomplete_group() {
        // Assistant has call-1 but the tool result has other-call.
        // New behavior: keep the assistant (model still sees what it tried)
        // and remove the mismatched tool result (which is orphan).
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            call("call-1"),
            result("other-call"),
            json!({"role": "user", "content": "continue"}),
        ];
        sanitize_tool_message_groups(&mut messages);
        assert_eq!(messages.len(), 3, "system + assistant kept + user kept; mismatched tool removed");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["role"], "user");
    }

    #[test]
    fn tolerates_interleaved_system_messages_between_tool_calls_and_results() {
        // RMOS injections and diagnostics are inserted between assistant
        // tool_calls and the tool results. The sanitizer should skip them.
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            call("call-1"),
            json!({"role": "system", "content": "rmos injection"}),
            json!({"role": "user", "content": "diagnostic feedback"}),
            result("call-1"),
        ];
        sanitize_tool_message_groups(&mut messages);
        assert_eq!(messages.len(), 5, "all five messages preserved — group complete with interleaved system/user");
    }

    #[test]
    fn removes_tool_result_with_no_matching_assistant_anywhere() {
        // call-1's group is complete (call + result). "orphan" has no
        // assistant anywhere and comes after the complete group — it is
        // removed.
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "question"}),
            call("call-1"),
            result("call-1"),  // matches call-1 → group complete
            result("orphan"),   // no assistant has this id → removed
            json!({"role": "user", "content": "latest"}),
        ];
        sanitize_tool_message_groups(&mut messages);
        assert_eq!(messages.len(), 5, "orphan result after complete group removed, others kept");
        assert!(!messages.iter().any(|m| {
            m.get("tool_call_id").and_then(Value::as_str) == Some("orphan")
        }));
    }

    #[test]
    fn context_trimming_never_splits_tool_group() {
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            json!({"role": "user", "content": "old"}),
            call("call-1"),
            result("call-1"),
            json!({"role": "user", "content": "latest"}),
        ];
        trim_api_messages_preserving_tool_groups(&mut messages, 80);
        let has_call = messages
            .iter()
            .any(|message| message.get("tool_calls").is_some());
        let has_result = messages.iter().any(|message| message["role"] == "tool");
        assert_eq!(has_call, has_result);
        assert_eq!(messages.last().unwrap()["content"], "latest");
    }
}
