//! LLM-03: ModelGateway using AiModelFactory (ai-server).
use crate::conversation::ports::{ConversationModelGateway, ModelToolCall, ModelTurnOutput};
use async_trait::async_trait;
use kairos_platform_ai_api::ChatMessage;
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{ConversationContext, ConversationIntent, ModelUsage};
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
        let project_id: Option<String> =
            sqlx::query_scalar("SELECT project_id FROM pent_agent_sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        let binding: Option<(i64, Option<f32>, Option<i32>)> = sqlx::query_as(
            "SELECT model_config_id, temperature, max_output_tokens
             FROM pent_model_role_bindings
             WHERE tenant_id = 0 AND model_role = 'conversation' AND enabled = true
               AND project_id IN ($1, '')
             ORDER BY CASE WHEN project_id = $1 THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(project_id.unwrap_or_default())
        .fetch_optional(&self.pool)
        .await?;
        if let Some((id, temperature, max_tokens)) = binding {
            return Ok((
                id,
                temperature.map(f64::from),
                max_tokens.unwrap_or(2000).clamp(1, 16_000) as u32,
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
        Ok((id, Some(0.3), 2000))
    }
}

#[async_trait]
impl ConversationModelGateway for AiModelGateway {
    async fn generate(
        &self,
        context: &ConversationContext,
        _intent: &ConversationIntent,
    ) -> anyhow::Result<ModelTurnOutput> {
        let (model_id, temperature, max_tokens) = self.resolve_model(&context.session_id).await?;
        let mut messages = vec![ChatMessage {
            role: "system".into(),
            content: context.system_instruction.clone(),
        }];
        messages.extend(context.recent_messages.iter().map(|message| ChatMessage {
            role: message.role.clone(),
            content: message.content.clone(),
        }));
        let started = std::time::Instant::now();

        let tools = super::tool_catalog::load_conversation_tool_definitions(&self.pool).await?;
        let mut api_messages: Vec<Value> =
            messages.iter().map(|m| format_message_for_api(m)).collect();

        // Safety: strip trailing unpaired tool_calls to avoid API errors.
        // This can happen when a previous turn was interrupted mid-tool-execution.
        strip_unpaired_tool_calls(&mut api_messages);
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

/// Remove any assistant messages with tool_calls that lack corresponding tool results.
/// DeepSeek/OpenAI APIs reject mismatched tool_calls ↔ tool_result pairs.
/// This walks backwards through ALL tool_calls groups, stripping any that are unpaired.
fn strip_unpaired_tool_calls(api_messages: &mut Vec<Value>) {
    // Find all positions of assistant tool_calls messages
    let positions: Vec<usize> = api_messages
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.get("role").and_then(Value::as_str) == Some("assistant")
                && m.get("tool_calls").is_some()
        })
        .map(|(i, _)| i)
        .collect();

    // Walk backwards — if any group is unpaired, strip from that point
    for &pos in positions.iter().rev() {
        let expected: usize = api_messages[pos]["tool_calls"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        // Count tool messages between this tool_calls and the NEXT assistant/user message
        let next_boundary = api_messages[pos + 1..]
            .iter()
            .position(|m| {
                let role = m.get("role").and_then(Value::as_str).unwrap_or("");
                role == "assistant" || role == "user" || role == "system"
            })
            .map(|p| pos + 1 + p)
            .unwrap_or(api_messages.len());
        let tool_msgs_in_group: usize = api_messages[pos + 1..next_boundary]
            .iter()
            .filter(|m| m.get("role").and_then(Value::as_str) == Some("tool"))
            .count();
        if tool_msgs_in_group < expected {
            api_messages.truncate(pos);
            tracing::warn!(
                expected,
                actual = tool_msgs_in_group,
                pos,
                "stripped unpaired tool_calls group, truncating context"
            );
        }
    }
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
