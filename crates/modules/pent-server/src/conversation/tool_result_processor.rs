use kairos_platform_pent_api::AgentMessage;
use kairos_platform_pent_runtime::observation::ObservationProjector;
use kairos_platform_pent_runtime::ports::{ConversationEventSink, ConversationRepository};
use kairos_platform_pent_runtime::tool_execution::CoordinatedToolResult;

use super::tool_result_sinks::{
    ConversationEventSinkAdapter, EvidenceSink, RunEventSink, TrafficSink,
};
pub(crate) use super::tool_result_sinks::{
    ToolResultPersistence, is_evidence_tool, is_finding_tool, is_traffic_tool,
};

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
        let mut projected = ObservationProjector::default().project(item);
        let evidence_id = EvidenceSink::new(self.repository)
            .persist(item, context.run_id, context.persistence)
            .await?;
        if let Some(evidence_id) = evidence_id.as_ref() {
            projected.add_evidence_id(evidence_id.clone());
        }

        let traffic_ids = TrafficSink::new(self.repository)
            .capture(item, context.run_id, context.capture_http_traffic)
            .await?;
        for traffic_id in traffic_ids {
            projected.add_traffic_id(traffic_id);
        }

        let event_payload = serde_json::json!({
            "turn_id": context.turn_id,
            "tool_name": item.tool_name,
            "tool_call_id": item.call_id,
            "success": item.result.success,
            "status": projected.normalized.status,
            "error": projected.normalized.error,
            "references": projected.normalized.references,
            "truncated": projected.normalized.truncated,
            "summary": item.result.content.chars().take(300).collect::<String>(),
        });
        let conversation_sink =
            ConversationEventSinkAdapter::new(self.repository, self.events, self.nats);
        conversation_sink
            .emit_completion(context.turn_id, &event_payload, context.emit_completion)
            .await;

        let display = serde_json::json!({
            "tool_name": item.tool_name,
            "tool_call_id": item.call_id,
            "success": item.result.success,
            "status": projected.normalized.status,
            "error": projected.normalized.error,
            "references": projected.normalized.references,
            "truncated": projected.normalized.truncated,
            "result": item.result.content,
        });
        conversation_sink
            .write_message(context.turn_id, context.session_id, &display)
            .await?;
        RunEventSink::new(self.repository)
            .record(context.run_id, context.turn_id, &display)
            .await?;

        Ok(ProcessedToolResult {
            succeeded: projected.succeeded,
            effective_observation: projected.effective,
            evidence_id,
            model_message: AgentMessage {
                message_id: context.message_id,
                role: "tool".into(),
                content: serde_json::json!({
                    "tool_call_id": item.call_id,
                    "status": projected.normalized.status,
                    "error": projected.normalized.error,
                    "references": projected.normalized.references,
                    "result": projected.model_content,
                })
                .to_string(),
                format: "tool_result".into(),
            },
        })
    }
}
