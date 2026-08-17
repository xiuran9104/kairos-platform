//! Infrastructure sinks for projected tool results.

use kairos_platform_pent_runtime::ports::{ConversationEventSink, ConversationRepository};
use kairos_platform_pent_runtime::tool_execution::CoordinatedToolResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultPersistence {
    MainEvidence,
    ExternalBestEffort,
}

pub struct EvidenceSink<'a> {
    repository: &'a dyn ConversationRepository,
}

impl<'a> EvidenceSink<'a> {
    pub const fn new(repository: &'a dyn ConversationRepository) -> Self {
        Self { repository }
    }

    pub async fn persist(
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

pub struct TrafficSink<'a> {
    repository: &'a dyn ConversationRepository,
}

impl<'a> TrafficSink<'a> {
    pub const fn new(repository: &'a dyn ConversationRepository) -> Self {
        Self { repository }
    }

    pub async fn capture(
        &self,
        item: &CoordinatedToolResult,
        run_id: Option<&str>,
        enabled: bool,
    ) -> anyhow::Result<Vec<String>> {
        if !enabled || !matches!(item.tool_name.as_str(), "http_request" | "http_probe") {
            return Ok(Vec::new());
        }
        let Some(run_id) = run_id else {
            return Ok(Vec::new());
        };
        self.repository
            .record_http_traffic(
                run_id,
                &item.call_id,
                &item.tool_name,
                &item.arguments,
                &item.result.content,
                None,
            )
            .await
    }
}

pub struct ConversationEventSinkAdapter<'a> {
    repository: &'a dyn ConversationRepository,
    events: Option<&'a dyn ConversationEventSink>,
    nats: Option<&'a crate::transport::mq::NatsHandle>,
}

impl<'a> ConversationEventSinkAdapter<'a> {
    pub const fn new(
        repository: &'a dyn ConversationRepository,
        events: Option<&'a dyn ConversationEventSink>,
        nats: Option<&'a crate::transport::mq::NatsHandle>,
    ) -> Self {
        Self {
            repository,
            events,
            nats,
        }
    }

    pub async fn emit_completion(
        &self,
        turn_id: Option<&str>,
        payload: &serde_json::Value,
        enabled: bool,
    ) {
        if !enabled {
            return;
        }
        if let Some(events) = self.events {
            events
                .emit("conversation.tool.completed", payload.clone())
                .await
                .ok();
        }
        if let (Some(nats), Some(turn_id)) = (self.nats, turn_id) {
            let payload = serde_json::to_vec(payload).unwrap_or_default();
            nats.publish(&format!("penta.conversation.tools.{turn_id}"), &payload)
                .await;
        }
    }

    pub async fn write_message(
        &self,
        turn_id: Option<&str>,
        session_id: &str,
        display: &serde_json::Value,
    ) -> anyhow::Result<()> {
        if let Some(turn_id) = turn_id {
            self.repository
                .write_intermediate_message(
                    turn_id,
                    session_id,
                    "tool",
                    &display.to_string(),
                    "tool_result",
                )
                .await?;
        }
        Ok(())
    }
}

pub struct RunEventSink<'a> {
    repository: &'a dyn ConversationRepository,
}

impl<'a> RunEventSink<'a> {
    pub const fn new(repository: &'a dyn ConversationRepository) -> Self {
        Self { repository }
    }

    pub async fn record(
        &self,
        run_id: Option<&str>,
        turn_id: Option<&str>,
        display: &serde_json::Value,
    ) -> anyhow::Result<()> {
        if turn_id.is_some()
            && let Some(run_id) = run_id
        {
            self.repository
                .record_run_event(run_id, "tool.executed", display)
                .await?;
        }
        Ok(())
    }
}

pub(crate) fn is_evidence_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "evidence_list"
            | "evidence_search"
            | "evidence_view"
            | "query_execution_result"
            | "query-execution-result"
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
mod execution_result_tests {
    use super::is_evidence_tool;

    #[test]
    fn execution_result_names_route_to_run_scoped_evidence_store() {
        assert!(is_evidence_tool("query_execution_result"));
        assert!(is_evidence_tool("query-execution-result"));
    }
}
