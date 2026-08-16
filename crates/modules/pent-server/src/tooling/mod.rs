//! Tool dispatch, execution backends and ports.

pub mod builtin;
pub mod dispatcher;
pub mod executors;
pub mod http;
pub mod mcp;
/// Compatibility facade for server adapters using the historical path.
pub mod ports {
    pub use kairos_platform_pent_tooling::ports::*;
}
pub mod reranker;
