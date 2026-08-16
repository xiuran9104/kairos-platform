//! Runtime ports implemented by server-side adapters.

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

#[derive(Debug, Clone)]
pub struct RuntimeToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub scope_snapshot: serde_json::Value,
    pub session_id: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    /// JSON-encoded observation returned to the model and evidence pipeline.
    pub content: String,
    pub success: bool,
}

#[async_trait]
pub trait ConversationToolExecutor: Send + Sync {
    async fn execute(
        &self,
        call: &RuntimeToolCall,
        context: &ToolExecutionContext,
    ) -> RuntimeToolResult;
}

#[derive(Debug, Clone)]
pub struct ToolApprovalRequest {
    pub run_id: String,
    pub session_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolApprovalOutcome {
    Approved(String),
    Rejected(String),
}

#[async_trait]
pub trait ToolApprovalGate: Send + Sync {
    async fn review(&self, request: &ToolApprovalRequest) -> anyhow::Result<ToolApprovalOutcome>;
}

#[async_trait]
pub trait FindingRecorder: Send + Sync {
    async fn record(&self, run_id: &str, arguments: &str) -> anyhow::Result<serde_json::Value>;
    async fn reconcile(&self, run_id: &str, session_id: &str) -> anyhow::Result<usize>;
}

#[async_trait]
pub trait PlanStore: Send + Sync {
    async fn exists(&self, run_id: &str) -> anyhow::Result<bool>;
    async fn execute(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<serde_json::Value>;
}

#[derive(Debug, Clone)]
pub struct RuntimeAgentProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instruction: String,
}

pub trait RoleResolver: Send + Sync {
    fn resolve(&self, role: &str) -> anyhow::Result<Option<RuntimeAgentProfile>>;
}

#[async_trait]
pub trait SkillStore: Send + Sync {
    async fn execute(&self, tool_name: &str, arguments: &str) -> anyhow::Result<serde_json::Value>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKnowledgeLayer {
    Technology,
    Vulnerability,
    Methodology,
    ToolUsage,
    ExploitCase,
    Remediation,
}

impl RuntimeKnowledgeLayer {
    pub fn layers_for_technology() -> Vec<Self> {
        vec![Self::Technology, Self::ExploitCase]
    }

    pub fn layers_for_vulnerability() -> Vec<Self> {
        vec![Self::Vulnerability, Self::ExploitCase, Self::Remediation]
    }

    pub fn layers_for_methodology() -> Vec<Self> {
        vec![Self::Methodology]
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeKnowledgeBlock {
    pub title: String,
    pub layer: String,
    pub content: String,
}

#[async_trait]
pub trait RuntimeEnrichment: Send + Sync {
    async fn search_context_knowledge(
        &self,
        query: &str,
        layers: &[RuntimeKnowledgeLayer],
        limit: usize,
    ) -> anyhow::Result<Vec<RuntimeKnowledgeBlock>>;

    async fn search_memories(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<String>>;

    async fn analyze_image(
        &self,
        image_data_url: &str,
        question: Option<&str>,
    ) -> anyhow::Result<String>;

    async fn log_knowledge_retrieval(
        &self,
        query: &str,
        guides: &[KnowledgeGuide],
        session_id: &str,
        run_id: Option<&str>,
    ) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub struct AgentSpawnRequest {
    pub description: String,
    pub prompt: String,
    pub session_id: String,
    pub parent_run_id: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeSubAgent {
    pub agent_id: String,
    pub description: String,
    pub status: RuntimeAgentStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

pub trait AgentSessionFactory: Send + Sync {
    fn create(&self, max_concurrent: usize) -> std::sync::Arc<dyn AgentSession>;
}

#[async_trait]
pub trait AgentSession: Send + Sync {
    async fn active_count(&self) -> usize;
    async fn spawn(&self, request: AgentSpawnRequest) -> anyhow::Result<String>;
    async fn status(&self, agent_id: &str) -> Option<RuntimeSubAgent>;
    async fn collect(&self, agent_id: &str) -> Option<String>;
    async fn list(&self) -> Vec<RuntimeSubAgent>;
    async fn abort_all(&self);
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

    async fn record_run_event(
        &self,
        run_id: &str,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()>;
}

#[async_trait]
pub trait ConversationContextSource: Send + Sync {
    async fn build(&self, request: &AgentTurnRequest) -> anyhow::Result<ConversationContext>;
}

#[async_trait]
pub trait ConversationIntentDetector: Send + Sync {
    async fn detect(
        &self,
        request: &AgentTurnRequest,
        run_active: bool,
    ) -> anyhow::Result<ConversationIntent>;
}

use std::collections::HashSet;

#[async_trait]
pub trait ConversationModelGateway: Send + Sync {
    async fn generate(
        &self,
        context: &ConversationContext,
        intent: &ConversationIntent,
    ) -> anyhow::Result<ModelTurnOutput>;

    /// Temporarily hide tools that have exceeded their consecutive failure
    /// limit. Called by the runtime before each model turn.
    fn set_exhausted_tools(&self, _tools: HashSet<String>) {}
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

#[cfg(test)]
mod tests {
    use super::*;

    struct StubAgentSession;

    #[async_trait]
    impl AgentSession for StubAgentSession {
        async fn active_count(&self) -> usize {
            0
        }

        async fn spawn(&self, _request: AgentSpawnRequest) -> anyhow::Result<String> {
            Ok("sub-test".into())
        }

        async fn status(&self, _agent_id: &str) -> Option<RuntimeSubAgent> {
            None
        }

        async fn collect(&self, _agent_id: &str) -> Option<String> {
            None
        }

        async fn list(&self) -> Vec<RuntimeSubAgent> {
            Vec::new()
        }

        async fn abort_all(&self) {}
    }

    struct StubAgentSessionFactory;

    impl AgentSessionFactory for StubAgentSessionFactory {
        fn create(&self, _max_concurrent: usize) -> std::sync::Arc<dyn AgentSession> {
            std::sync::Arc::new(StubAgentSession)
        }
    }

    struct StubToolExecutor;

    struct StubToolApprovalGate;

    struct StubFindingRecorder;

    #[async_trait]
    impl FindingRecorder for StubFindingRecorder {
        async fn record(
            &self,
            _run_id: &str,
            _arguments: &str,
        ) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::json!({"recorded": true}))
        }

        async fn reconcile(&self, _run_id: &str, _session_id: &str) -> anyhow::Result<usize> {
            Ok(0)
        }
    }

    struct StubPlanStore;

    #[async_trait]
    impl PlanStore for StubPlanStore {
        async fn exists(&self, _run_id: &str) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn execute(
            &self,
            _run_id: &str,
            _tool_name: &str,
            _arguments: &str,
        ) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::json!({}))
        }
    }

    struct StubRoleResolver;

    impl RoleResolver for StubRoleResolver {
        fn resolve(&self, _role: &str) -> anyhow::Result<Option<RuntimeAgentProfile>> {
            Ok(None)
        }
    }

    struct StubSkillStore;

    #[async_trait]
    impl SkillStore for StubSkillStore {
        async fn execute(
            &self,
            _tool_name: &str,
            _arguments: &str,
        ) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::json!({}))
        }
    }

    struct StubRuntimeEnrichment;

    #[async_trait]
    impl RuntimeEnrichment for StubRuntimeEnrichment {
        async fn search_context_knowledge(
            &self,
            _query: &str,
            _layers: &[RuntimeKnowledgeLayer],
            _limit: usize,
        ) -> anyhow::Result<Vec<RuntimeKnowledgeBlock>> {
            Ok(Vec::new())
        }

        async fn search_memories(
            &self,
            _query: &str,
            _top_k: usize,
        ) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn analyze_image(
            &self,
            _image_data_url: &str,
            _question: Option<&str>,
        ) -> anyhow::Result<String> {
            Ok(String::new())
        }

        async fn log_knowledge_retrieval(
            &self,
            _query: &str,
            _guides: &[KnowledgeGuide],
            _session_id: &str,
            _run_id: Option<&str>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ToolApprovalGate for StubToolApprovalGate {
        async fn review(
            &self,
            request: &ToolApprovalRequest,
        ) -> anyhow::Result<ToolApprovalOutcome> {
            Ok(ToolApprovalOutcome::Approved(request.arguments.clone()))
        }
    }

    #[async_trait]
    impl ConversationToolExecutor for StubToolExecutor {
        async fn execute(
            &self,
            call: &RuntimeToolCall,
            _context: &ToolExecutionContext,
        ) -> RuntimeToolResult {
            RuntimeToolResult {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                content: "{}".into(),
                success: true,
            }
        }
    }

    #[test]
    fn conversation_tool_executor_is_object_safe() {
        let executor = StubToolExecutor;
        let _: &dyn ConversationToolExecutor = &executor;
    }

    #[test]
    fn tool_approval_gate_is_object_safe() {
        let gate = StubToolApprovalGate;
        let _: &dyn ToolApprovalGate = &gate;
    }

    #[test]
    fn persistence_ports_are_object_safe() {
        let recorder = StubFindingRecorder;
        let _: &dyn FindingRecorder = &recorder;
        let plans = StubPlanStore;
        let _: &dyn PlanStore = &plans;
    }

    #[test]
    fn capability_ports_are_object_safe() {
        let roles = StubRoleResolver;
        let _: &dyn RoleResolver = &roles;
        let skills = StubSkillStore;
        let _: &dyn SkillStore = &skills;
        let enrichment = StubRuntimeEnrichment;
        let _: &dyn RuntimeEnrichment = &enrichment;
    }

    #[test]
    fn agent_session_ports_are_object_safe() {
        let factory = StubAgentSessionFactory;
        let _: &dyn AgentSessionFactory = &factory;
        let session = factory.create(2);
        let _: &dyn AgentSession = session.as_ref();
    }
}
