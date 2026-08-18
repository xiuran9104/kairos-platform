//! Conversation runtime ports and orchestration.

mod finding_projection;
mod runtime;
mod web_coverage;
mod web_session;
mod async_tools;

pub use kairos_platform_pent_runtime::ports::*;
pub use runtime::*;
use std::sync::Arc;

use adapters::{DeterministicIntentDetector, LoggingConversationEventSink};
use context_source::PgContextSource;
use instruction_sink::InstructionSink;
use memory_adapter::MemoryAdapter;
use model_gateway::AiModelGateway;
use repository_postgres::PgConversationRepository;

pub(crate) fn build_runtime(
    pool: &kairos_platform_framework_database::PgPool,
    nats: Option<crate::transport::mq::NatsHandle>,
) -> ConversationRuntime {
    let repository: Arc<dyn ConversationRepository> =
        Arc::new(PgConversationRepository::new(pool.clone()));
    let context_source: Arc<dyn ConversationContextSource> =
        Arc::new(PgContextSource::new(pool.clone()));
    let intent_detector: Arc<dyn ConversationIntentDetector> =
        Arc::new(DeterministicIntentDetector);
    let model_gateway: Arc<dyn ConversationModelGateway> =
        Arc::new(AiModelGateway::new(pool.clone()));
    let knowledge_source: Arc<dyn ConversationKnowledgeSource> =
        Arc::new(MemoryAdapter::new(pool.clone()));
    let instruction_sink: Arc<dyn ConversationInstructionSink> =
        Arc::new(InstructionSink::new(pool.clone()));
    let tool_executor: Arc<dyn ConversationToolExecutor> =
        Arc::new(tool_executor::ServerConversationToolExecutor);
    let approval_gate: Arc<dyn ToolApprovalGate> = Arc::new(hitl::HitlGate::new(pool.clone()));
    let finding_recorder: Arc<dyn FindingRecorder> =
        Arc::new(run_bridge::PgFindingRecorder::new(pool.clone()));
    let plan_store: Arc<dyn PlanStore> = Arc::new(plan_board::PgPlanStore::new(pool.clone()));
    let role_resolver: Arc<dyn RoleResolver> = Arc::new(agent_profiles::MarkdownRoleResolver);
    let skill_store: Arc<dyn SkillStore> = Arc::new(skill_store::PgSkillStore::new(pool.clone()));
    let enrichment: Arc<dyn RuntimeEnrichment> =
        Arc::new(enrichment_adapter::PgRuntimeEnrichment::new(pool.clone()));
    let events: Arc<dyn ConversationEventSink> = Arc::new(LoggingConversationEventSink);
    let agent_sessions: Arc<dyn AgentSessionFactory> =
        Arc::new(agent_manager::ServerAgentSessionFactory::new(
            pool.clone(),
            repository.clone(),
            model_gateway.clone(),
            context_source.clone(),
            knowledge_source.clone(),
            tool_executor.clone(),
            approval_gate.clone(),
            finding_recorder.clone(),
            skill_store.clone(),
            intent_detector.clone(),
        ));
    let rt = ConversationRuntime::new(
        repository,
        context_source,
        intent_detector,
        model_gateway,
        knowledge_source,
        instruction_sink,
        tool_executor,
        approval_gate,
        finding_recorder,
        plan_store,
        role_resolver,
        skill_store,
        enrichment,
        agent_sessions,
        events,
    );
    if let Some(n) = nats {
        rt.with_nats(n)
    } else {
        rt
    }
}

/// Build the same governed sub-agent scheduler used by conversation turns for
/// non-conversation entry points such as Workflow execution.
pub(crate) fn build_agent_session(
    pool: &kairos_platform_framework_database::PgPool,
    max_concurrent: usize,
) -> Arc<dyn AgentSession> {
    let repository: Arc<dyn ConversationRepository> =
        Arc::new(PgConversationRepository::new(pool.clone()));
    let context_source: Arc<dyn ConversationContextSource> =
        Arc::new(PgContextSource::new(pool.clone()));
    let intent_detector: Arc<dyn ConversationIntentDetector> =
        Arc::new(DeterministicIntentDetector);
    let model_gateway: Arc<dyn ConversationModelGateway> =
        Arc::new(AiModelGateway::new(pool.clone()));
    let knowledge_source: Arc<dyn ConversationKnowledgeSource> =
        Arc::new(MemoryAdapter::new(pool.clone()));
    let tool_executor: Arc<dyn ConversationToolExecutor> =
        Arc::new(tool_executor::ServerConversationToolExecutor);
    let approval_gate: Arc<dyn ToolApprovalGate> = Arc::new(hitl::HitlGate::new(pool.clone()));
    let finding_recorder: Arc<dyn FindingRecorder> =
        Arc::new(run_bridge::PgFindingRecorder::new(pool.clone()));
    let skill_store: Arc<dyn SkillStore> = Arc::new(skill_store::PgSkillStore::new(pool.clone()));
    let factory = agent_manager::ServerAgentSessionFactory::new(
        pool.clone(),
        repository,
        model_gateway,
        context_source,
        knowledge_source,
        tool_executor,
        approval_gate,
        finding_recorder,
        skill_store,
        intent_detector,
    );
    factory.create(max_concurrent.max(1))
}

pub mod adapters;
pub mod agent_http;
pub(crate) mod agent_manager;
pub(crate) mod agent_pipeline;
pub(crate) mod agent_profiles;
pub(crate) mod claim_gate;
pub mod context_source;
mod enrichment_adapter;
pub mod error_codes;
pub mod file_cache;
pub mod hitl;
pub mod hitl_http;
pub mod http;
pub mod instruction_sink;
pub mod memory_adapter;
pub mod model_gateway;
pub(crate) mod plan_board;
pub mod repository_postgres;
pub mod recovery;
pub mod rmos;
pub(crate) mod run_bridge;
mod scan_progress;
mod skill_store;
pub mod stream;
pub mod structured_output;
pub mod tool_catalog;
pub mod tool_executor;
mod tool_extensions;
mod tool_guidance;
mod tool_health;
pub mod tool_guard;
mod tool_retrieval;
mod tool_result_processor;
mod tool_result_sinks;
pub mod traffic;
pub(crate) mod turn_tasks;
