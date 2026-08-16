use kairos_platform_pent_api::AgentMessage;
use kairos_platform_pent_runtime::ports::{ConversationEventSink, ConversationRepository};
use kairos_platform_pent_runtime::tool_result::ToolResultStatus;

use super::tool_execution_coordinator::CoordinatedToolResult;

const DEFAULT_MODEL_OBSERVATION_CHARS: usize = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultPersistence {
    MainEvidence,
    ExternalBestEffort,
}

pub struct ToolResultProcessingContext<'a> {
    pub run_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub session_id: &'a str,
    pub message_id: String,
    pub persistence: ToolResultPersistence,
    pub emit_completion: bool,
    pub capture_http_traffic: bool,
}

pub struct ProcessedToolResult {
    pub succeeded: bool,
    pub effective_observation: bool,
    pub evidence_id: Option<String>,
    pub model_message: AgentMessage,
}

pub struct ToolResultProcessor<'a> {
    repository: &'a dyn ConversationRepository,
    events: Option<&'a dyn ConversationEventSink>,
    nats: Option<&'a crate::transport::mq::NatsHandle>,
}

impl<'a> ToolResultProcessor<'a> {
    pub fn new(repository: &'a dyn ConversationRepository) -> Self {
        Self {
            repository,
            events: None,
            nats: None,
        }
    }

    pub fn with_events(mut self, events: &'a dyn ConversationEventSink) -> Self {
        self.events = Some(events);
        self
    }

    pub fn with_nats(mut self, nats: Option<&'a crate::transport::mq::NatsHandle>) -> Self {
        self.nats = nats;
        self
    }

    pub async fn process(
        &self,
        item: &CoordinatedToolResult,
        context: ToolResultProcessingContext<'_>,
    ) -> anyhow::Result<ProcessedToolResult> {
        let result = &item.result;
        let effective_observation = has_effective_observation(item);
        let mut normalized = item.normalized.clone();
        let evidence_id = self
            .persist_result(item, context.run_id, context.persistence)
            .await?;
        if let Some(evidence_id) = evidence_id.as_ref() {
            push_unique(&mut normalized.references.evidence_ids, evidence_id.clone());
        }

        if context.capture_http_traffic
            && matches!(item.tool_name.as_str(), "http_request" | "http_probe")
            && let Some(run_id) = context.run_id
        {
            let traffic_ids = self
                .repository
                .record_http_traffic(
                    run_id,
                    &item.call_id,
                    &item.tool_name,
                    &item.arguments,
                    &result.content,
                    None,
                )
                .await?;
            for traffic_id in traffic_ids {
                push_unique(&mut normalized.references.traffic_ids, traffic_id);
            }
        }

        let event_payload = serde_json::json!({
            "turn_id": context.turn_id,
            "tool_name": item.tool_name,
            "tool_call_id": item.call_id,
            "success": result.success,
            "status": normalized.status,
            "error": normalized.error,
            "references": normalized.references,
            "truncated": normalized.truncated,
            "summary": result.content.chars().take(300).collect::<String>(),
        });
        if context.emit_completion {
            if let Some(events) = self.events {
                events
                    .emit("conversation.tool.completed", event_payload.clone())
                    .await
                    .ok();
            }
            if let (Some(nats), Some(turn_id)) = (self.nats, context.turn_id) {
                let payload = serde_json::to_vec(&event_payload).unwrap_or_default();
                nats.publish(&format!("penta.conversation.tools.{turn_id}"), &payload)
                    .await;
            }
        }

        let display = serde_json::json!({
            "tool_name": item.tool_name,
            "tool_call_id": item.call_id,
            "success": result.success,
            "status": normalized.status,
            "error": normalized.error,
            "references": normalized.references,
            "truncated": normalized.truncated,
            "result": result.content,
        });
        if let Some(turn_id) = context.turn_id {
            self.repository
                .write_intermediate_message(
                    turn_id,
                    context.session_id,
                    "tool",
                    &display.to_string(),
                    "tool_result",
                )
                .await?;
            if let Some(run_id) = context.run_id {
                self.repository
                    .record_run_event(run_id, "tool.executed", &display)
                    .await?;
            }
        }

        Ok(ProcessedToolResult {
            succeeded: normalized.status == ToolResultStatus::Succeeded,
            effective_observation,
            evidence_id,
            model_message: AgentMessage {
                message_id: context.message_id,
                role: "tool".into(),
                content: serde_json::json!({
                    "tool_call_id": item.call_id,
                    "status": normalized.status,
                    "error": normalized.error,
                    "references": normalized.references,
                    "result": compact_model_observation(&result.content),
                })
                .to_string(),
                format: "tool_result".into(),
            },
        })
    }

    async fn persist_result(
        &self,
        item: &CoordinatedToolResult,
        run_id: Option<&str>,
        persistence: ToolResultPersistence,
    ) -> anyhow::Result<Option<String>> {
        let Some(run_id) = run_id else {
            return Ok(None);
        };
        let should_persist = match persistence {
            ToolResultPersistence::MainEvidence => {
                !is_evidence_tool(&item.tool_name)
                    && !is_traffic_tool(&item.tool_name)
                    && !is_finding_tool(&item.tool_name)
            }
            ToolResultPersistence::ExternalBestEffort => !item.inline,
        };
        if !should_persist {
            return Ok(None);
        }
        let persisted = self
            .repository
            .record_tool_result(
                run_id,
                &item.call_id,
                &item.tool_name,
                &item.arguments,
                item.result.success,
                &item.result.content,
                item.elapsed.as_millis().try_into().unwrap_or(u64::MAX),
            )
            .await;
        match (persistence, persisted) {
            (_, Ok(evidence_id)) => Ok(Some(evidence_id)),
            (ToolResultPersistence::ExternalBestEffort, Err(error)) => {
                tracing::warn!(%error, tool = %item.tool_name, "failed to persist sub-agent tool result");
                Ok(None)
            }
            (ToolResultPersistence::MainEvidence, Err(error)) => Err(error),
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

pub fn has_effective_observation(item: &CoordinatedToolResult) -> bool {
    if item.normalized.status != ToolResultStatus::Succeeded {
        return false;
    }
    let tool_name = item.tool_name.as_str();
    let content = item.result.content.as_str();
    if matches!(
        tool_name,
        "knowledge_search"
            | "evidence_list"
            | "evidence_search"
            | "evidence_view"
            | "traffic_list"
            | "traffic_view"
            | "traffic_sitemap"
            | "traffic_diff"
            | "skill_list"
            | "skill_load"
            | "record_finding"
            | "agent_spawn"
            | "agent_status"
            | "agent_collect"
            | "agent_list"
            | "plan_create"
            | "plan_get"
            | "plan_update"
    ) {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return !content.trim().is_empty();
    };
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

pub fn compact_model_observation(content: &str) -> String {
    if content.chars().count() <= DEFAULT_MODEL_OBSERVATION_CHARS {
        return content.to_string();
    }
    let prefix: String = content
        .chars()
        .take(DEFAULT_MODEL_OBSERVATION_CHARS)
        .collect();
    format!(
        "{prefix}\n...[模型上下文已截断；完整原始响应已保存为 Evidence，共 {} 字符]",
        content.chars().count()
    )
}

pub(crate) fn is_evidence_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "evidence_list" | "evidence_search" | "evidence_view"
    )
}

pub(crate) fn is_traffic_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "traffic_list" | "traffic_view" | "traffic_diff" | "traffic_replay" | "traffic_sitemap"
    )
}

pub(crate) fn is_finding_tool(tool_name: &str) -> bool {
    tool_name == "record_finding"
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use kairos_platform_pent_runtime::ports::RuntimeToolResult;
    use kairos_platform_pent_runtime::tool_result::normalize_tool_result;

    use super::*;

    fn item(tool_name: &str, success: bool, content: &str) -> CoordinatedToolResult {
        let result = RuntimeToolResult {
            tool_call_id: "call-1".into(),
            tool_name: tool_name.into(),
            content: content.into(),
            success,
        };
        let normalized = normalize_tool_result(&result);
        CoordinatedToolResult {
            index: 0,
            call_id: "call-1".into(),
            tool_name: tool_name.into(),
            arguments: "{}".into(),
            result,
            normalized,
            elapsed: Duration::ZERO,
            inline: false,
        }
    }

    #[test]
    fn reference_and_blocked_results_are_not_effective() {
        assert!(!has_effective_observation(&item(
            "knowledge_search",
            true,
            r#"{"guides":[{"title":"SQL injection"}]}"#,
        )));
        assert!(!has_effective_observation(&item(
            "dir_fuzz",
            false,
            r#"{"blocked":true,"error":"disabled"}"#,
        )));
    }

    #[test]
    fn target_observation_requires_meaningful_content() {
        assert!(!has_effective_observation(&item(
            "js_analyze",
            true,
            r#"{"endpoints":[],"security_hints":[],"technology_hints":[]}"#,
        )));
        assert!(has_effective_observation(&item(
            "js_analyze",
            true,
            r#"{"endpoints":["/api/users"],"security_hints":[]}"#,
        )));
    }

    #[test]
    fn model_observation_is_bounded() {
        let raw = "a".repeat(DEFAULT_MODEL_OBSERVATION_CHARS + 10);
        let compact = compact_model_observation(&raw);
        assert!(compact.contains("完整原始响应已保存为 Evidence"));
        assert!(compact.len() < raw.len() + 100);
    }
}
