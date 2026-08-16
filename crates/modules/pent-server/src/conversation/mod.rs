//! Conversation runtime ports and orchestration.

mod runtime;

/// Compatibility facade for callers that still import runtime ports through
/// `pent_server::conversation::ports`.
pub mod ports {
    pub use kairos_platform_pent_runtime::ports::*;
}

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
    let events: Arc<dyn ConversationEventSink> = Arc::new(LoggingConversationEventSink);
    let agent_sessions: Arc<dyn AgentSessionFactory> =
        Arc::new(agent_manager::ServerAgentSessionFactory::new(
            repository.clone(),
            model_gateway.clone(),
            context_source.clone(),
            knowledge_source.clone(),
            tool_executor.clone(),
            events.clone(),
            intent_detector.clone(),
            pool.clone(),
        ));
    let rt = ConversationRuntime::new(
        repository,
        context_source,
        intent_detector,
        model_gateway,
        knowledge_source,
        instruction_sink,
        tool_executor,
        agent_sessions,
        events,
        pool.clone(),
    );
    if let Some(n) = nats {
        rt.with_nats(n)
    } else {
        rt
    }
}

pub mod adapters;
pub mod agent_http;
pub(crate) mod agent_manager;
pub(crate) mod agent_pipeline;
pub(crate) mod agent_profiles;
pub(crate) mod claim_gate;
mod checkpoint;
pub(crate) mod plan_board;
/// Compatibility facade; context budgeting now belongs to `pent-runtime`.
pub mod context_guard {
    pub use kairos_platform_pent_runtime::context_guard::*;
}
pub mod context_source;
pub mod error_codes;
pub mod file_cache;
pub mod http;
pub mod hitl;
pub mod hitl_http;
pub mod instruction_sink;
pub mod memory_adapter;
pub mod model_gateway;
pub mod repository_postgres;
pub(crate) mod run_bridge;
pub mod rmos;
mod scan_progress;
pub mod stream;
pub mod structured_output;
pub mod tool_bus;
pub mod tool_catalog;
pub mod tool_executor;
pub mod tool_guard;
mod tool_extensions;
pub mod traffic;
pub(crate) mod turn_tasks;
