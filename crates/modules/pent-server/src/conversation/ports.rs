use async_trait::async_trait;
use kairos_platform_pent_api::{
    AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent, ModelUsage,
};

#[derive(Debug, Clone)]
pub struct ModelTurnOutput {
    pub content: String,
    pub reasoning_summary: Option<String>,
    pub plan_update: Option<kairos_platform_pent_api::ConversationPlanUpdate>,
    pub citations: Vec<kairos_platform_pent_api::ConversationCitation>,
    pub usage: ModelUsage,
    /// Tool calls extracted from the model response (if the model requested tool execution)
    pub tool_calls: Vec<ModelToolCall>,
}

#[derive(Debug, Clone)]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn begin_turn(
        &self,
        request: &AgentTurnRequest,
        intent: &ConversationIntent,
    ) -> anyhow::Result<(String, Option<String>)>;
    async fn complete_turn(&self, result: &AgentTurnResult) -> anyhow::Result<()>;
    async fn fail_turn(
        &self,
        turn_id: &str,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> anyhow::Result<()>;
    /// Write an intermediate message (tool call, tool result) so the SSE stream can relay it.
    async fn write_intermediate_message(
        &self,
        turn_id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        format: &str,
    ) -> anyhow::Result<()>;
}

#[async_trait]
pub trait ConversationContextSource: Send + Sync {
    async fn build(&self, request: &AgentTurnRequest) -> anyhow::Result<ConversationContext>;
}

#[async_trait]
pub trait ConversationAuthorizationService: Send + Sync {
    async fn reconcile_explicit_authorization(
        &self,
        session_id: &str,
        content: &str,
    ) -> anyhow::Result<bool>;
}

#[async_trait]
pub trait ConversationIntentDetector: Send + Sync {
    async fn detect(
        &self,
        request: &AgentTurnRequest,
        run_active: bool,
    ) -> anyhow::Result<ConversationIntent>;
}

#[async_trait]
pub trait ConversationModelGateway: Send + Sync {
    async fn generate(
        &self,
        context: &ConversationContext,
        intent: &ConversationIntent,
    ) -> anyhow::Result<ModelTurnOutput>;
}

#[async_trait]
pub trait ConversationInstructionSink: Send + Sync {
    async fn submit(
        &self,
        run_id: &str,
        request: &AgentTurnRequest,
        intent: &ConversationIntent,
    ) -> anyhow::Result<String>;
}

#[async_trait]
pub trait ConversationEventSink: Send + Sync {
    async fn emit(&self, event_type: &str, payload: serde_json::Value) -> anyhow::Result<()>;
}
