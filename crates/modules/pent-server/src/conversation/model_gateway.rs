//! LLM-03: ModelGateway using AiModelFactory (ai-server).
use crate::conversation::ports::{ConversationModelGateway, ModelToolCall, ModelTurnOutput};
use async_trait::async_trait;
use kairos_platform_ai_api::ChatMessage;
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{ConversationContext, ConversationIntent, ModelUsage};
use kairos_platform_pent_core::conversation::DEFAULT_CONTEXT_TOKEN_BUDGET;
use serde_json::Value;

pub struct AiModelGateway {
    pool: PgPool,
    factory: AiModelFactory,
}

impl AiModelGateway {
    pub fn new(pool: PgPool) -> Self {
        Self {
            factory: AiModelFactory::new(pool.clone()),
            pool,
        }
    }

    async fn resolve_model(&self, session_id: &str) -> anyhow::Result<(i64, Option<f64>, u32)> {
        let (tenant_id, project_id): (i64, Option<String>) =
            sqlx::query_as("SELECT tenant_id, project_id FROM pent_agent_sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Agent Session 不存在"))?;
        let binding: Option<(i64, Option<f32>, Option<i32>)> = sqlx::query_as(
            "SELECT model_config_id, temperature, max_output_tokens
             FROM pent_model_role_bindings
             WHERE tenant_id = $1 AND model_role = 'conversation' AND enabled = true
               AND project_id IN ($2, '')
             ORDER BY CASE WHEN project_id = $2 THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(project_id.unwrap_or_default())
        .fetch_optional(&self.pool)
        .await?;
        if let Some((id, temperature, max_tokens)) = binding {
            let configured_max = max_tokens.unwrap_or(2000).clamp(1, 16_000) as u32;
            return Ok((
                id,
                temperature.map(f64::from),
                conversation_output_token_limit(configured_max),
            ));
        }
        let id: i64 = sqlx::query_scalar(
            "SELECT id FROM ai.model_configs
             WHERE type = 'chat' AND status = 0
             ORDER BY id LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("未配置可用的聊天模型"))?;
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

#[async_trait]
impl ConversationModelGateway for AiModelGateway {
    async fn generate(
        &self,
        context: &ConversationContext,
        _intent: &ConversationIntent,
    ) -> anyhow::Result<ModelTurnOutput> {
        let (model_id, temperature, max_tokens) = self.resolve_model(&context.session_id).await?;
        let context_token_budget = std::env::var("PENTA_CONTEXT_TOKEN_BUDGET")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_CONTEXT_TOKEN_BUDGET)
            .clamp(2_000, 128_000);
        let model_context = context;
        let memory_context = if model_context.run_summary.trim().is_empty() {
            String::new()
        } else {
            format!("\n\n## 运行摘要与长期记忆\n{}", model_context.run_summary)
        };
        let mut messages = vec![ChatMessage {
            role: "system".into(),
            content: format!("{}{}", model_context.system_instruction, memory_context),
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

        let tools = super::tool_catalog::load_conversation_tool_definitions(&self.pool).await?;
        let mut api_messages: Vec<Value> =
            messages.iter().map(|m| format_message_for_api(m)).collect();

        // Context trimming or an interrupted turn can leave either side of the
        // tool protocol without its pair. Validate every group by tool_call_id.
        sanitize_tool_message_groups(&mut api_messages);
        trim_api_messages_preserving_tool_groups(&mut api_messages, context_token_budget);
        let response = self
            .factory
            .chat_tools(model_id, api_messages, tools, temperature, Some(max_tokens))
            .await
            .map_err(|error| anyhow::anyhow!("模型调用失败: {error:?}"))?;

        // Parse the response — it may contain either text or tool calls
        let (content, tool_calls) = parse_chat_tools_response(&response);

        if content.trim().is_empty() && tool_calls.is_empty() {
            anyhow::bail!("模型返回了空内容");
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
                "content": null,
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

/// Keep only complete `assistant.tool_calls` + consecutive `tool` response
/// groups. This removes orphan results anywhere in history, not just at the
/// beginning, and also removes interrupted assistant groups.
fn sanitize_tool_message_groups(api_messages: &mut Vec<Value>) {
    let source = std::mem::take(api_messages);
    let mut sanitized = Vec::with_capacity(source.len());
    let mut index = 0;

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
            let mut group_end = index + 1;
            while group_end < source.len()
                && source[group_end].get("role").and_then(Value::as_str) == Some("tool")
            {
                group_end += 1;
            }
            let actual: Vec<&str> = source[index + 1..group_end]
                .iter()
                .filter_map(|result| result.get("tool_call_id").and_then(Value::as_str))
                .collect();
            let actual_set: std::collections::HashSet<&str> = actual.iter().copied().collect();
            let complete =
                !expected.is_empty() && actual.len() == expected.len() && actual_set == expected;

            if complete {
                sanitized.extend(source[index..group_end].iter().cloned());
            } else {
                tracing::warn!(
                    expected = expected.len(),
                    actual = actual.len(),
                    "removed incomplete tool message group from model context"
                );
            }
            index = group_end;
            continue;
        }

        if role == Some("tool") {
            tracing::warn!("removed orphan tool result from model context");
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
    use super::{sanitize_tool_message_groups, trim_api_messages_preserving_tool_groups};
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
        let mut messages = vec![
            json!({"role": "system", "content": "s"}),
            call("call-1"),
            result("other-call"),
            json!({"role": "user", "content": "continue"}),
        ];
        sanitize_tool_message_groups(&mut messages);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["role"], "user");
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
