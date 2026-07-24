use async_trait::async_trait;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationIntent};

use super::{ConversationEventSink, ConversationIntentDetector};

pub struct DeterministicIntentDetector;

#[async_trait]
impl ConversationIntentDetector for DeterministicIntentDetector {
    async fn detect(
        &self,
        request: &AgentTurnRequest,
        run_active: bool,
    ) -> anyhow::Result<ConversationIntent> {
        Ok(kairos_platform_pent_core::conversation::classify_intent(
            &request.content,
            run_active,
        ))
    }
}

/// Conversation state is persisted by the repository. Domain events become
/// run events only after a run exists, so a chat-only session needs no extra row.
pub struct LoggingConversationEventSink;

#[async_trait]
impl ConversationEventSink for LoggingConversationEventSink {
    async fn emit(&self, event_type: &str, payload: serde_json::Value) -> anyhow::Result<()> {
        tracing::debug!(event_type, payload = %payload, "penta conversation event");
        Ok(())
    }
}
