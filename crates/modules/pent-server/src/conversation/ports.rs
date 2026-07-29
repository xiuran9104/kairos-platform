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

#[derive(Debug, Clone, serde::Serialize)]
pub struct KnowledgeGuide {
    pub id: String,
    pub title: String,
    pub source: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct HistoricalToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub success: bool,
    /// Semantic result fingerprint persisted at record time; legacy rows may
    /// only carry the exact content hash (prefixed with the tool name).
    pub semantic_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RunToolHistory {
    pub evidence_ids: Vec<String>,
    pub tool_calls: Vec<HistoricalToolCall>,
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn begin_turn(
        &self,
        request: &AgentTurnRequest,
        intent: &ConversationIntent,
    ) -> anyhow::Result<(String, Option<String>)>;
    async fn ensure_execution_run(
        &self,
        turn_id: &str,
        session_id: &str,
    ) -> anyhow::Result<Option<String>>;
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
    async fn record_tool_result(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
        success: bool,
        content: &str,
        duration_ms: u64,
    ) -> anyhow::Result<String>;
    /// Restore bounded execution state for duplicate prevention and completion
    /// gates when the same run continues in a later conversation turn.
    async fn load_run_tool_history(&self, run_id: &str) -> anyhow::Result<RunToolHistory>;
    /// Execute one of the read-only Evidence tools. Implementations must scope
    /// every query to `run_id`, redact secrets, and bound returned content.
    async fn execute_evidence_tool(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<serde_json::Value>;
    /// Load raw evidence texts for server-side claim projection checks.
    /// Returns `(evidence_id, source)` pairs scoped to `run_id`; only evidence
    /// present in the run is returned, so callers can trust the ids.
    async fn evidence_texts(
        &self,
        run_id: &str,
        evidence_ids: &[String],
    ) -> anyhow::Result<Vec<(String, String)>>;
    /// Execute one of the Skill tools (`skill_list`, `skill_load`). Only
    /// published skills are visible; skills never widen scope.
    async fn execute_skill_tool(
        &self,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<serde_json::Value>;
    /// Persist HTTP exchanges captured from `http_request` / `http_probe`
    /// conversation tool results. Returns the ids of the inserted rows.
    async fn record_http_traffic(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
        content: &str,
        replay_of: Option<&str>,
    ) -> anyhow::Result<Vec<String>>;
    /// Execute one of the read-only traffic tools (`traffic_list`,
    /// `traffic_view`, `traffic_diff`), scoped to `run_id`.
    async fn execute_traffic_tool(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<serde_json::Value>;
    /// Load a stored traffic exchange as `http_request` arguments for replay.
    async fn load_traffic_request(
        &self,
        run_id: &str,
        traffic_id: &str,
    ) -> anyhow::Result<serde_json::Value>;
    /// Persist the rolling run summary and the exact context references used to
    /// resume the next turn.
    async fn checkpoint_context(
        &self,
        turn_id: &str,
        context: &ConversationContext,
        run_summary: &str,
        evidence_ids: &[String],
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
pub trait ConversationKnowledgeSource: Send + Sync {
    async fn search_guides(&self, query: &str, limit: usize)
    -> anyhow::Result<Vec<KnowledgeGuide>>;
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
