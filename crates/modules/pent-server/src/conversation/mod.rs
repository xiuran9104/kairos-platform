//! Conversation runtime ports and orchestration.

mod ports;
mod runtime;

pub use ports::*;
pub use runtime::*;
use std::sync::Arc;

use adapters::{DeterministicIntentDetector, LoggingConversationEventSink};
use authorization::PgConversationAuthorizationService;
use context_source::PgContextSource;
use instruction_sink::InstructionSink;
use memory_adapter::MemoryAdapter;
use model_gateway::AiModelGateway;
use repository_postgres::PgConversationRepository;

pub(crate) fn build_runtime(
    pool: &kairos_platform_framework_database::PgPool,
) -> ConversationRuntime {
    ConversationRuntime::new(
        Arc::new(PgConversationRepository::new(pool.clone())),
        Arc::new(PgConversationAuthorizationService::new(pool.clone())),
        Arc::new(PgContextSource::new(pool.clone())),
        Arc::new(DeterministicIntentDetector),
        Arc::new(AiModelGateway::new(pool.clone())),
        Arc::new(MemoryAdapter::new(pool.clone())),
        Arc::new(InstructionSink::new(pool.clone())),
        Arc::new(LoggingConversationEventSink),
    )
}

pub mod adapters;
mod authorization;
pub mod context_source;
pub mod error_codes;
pub mod http;
pub mod instruction_sink;
pub mod memory_adapter;
pub mod model_gateway;
pub mod repository_postgres;
pub(crate) mod run_bridge;
mod scan_progress;
pub mod stream;
pub mod structured_output;
pub mod tool_catalog;
pub mod tool_executor;
mod tool_extensions;
mod tool_guard;
pub(crate) mod turn_tasks;
