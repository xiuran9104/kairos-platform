//! MCP client infrastructure, registry, protocol contracts, and executor.

pub mod client;
pub mod executor;
pub mod protocol;
pub mod recipes;
pub mod registry;
pub mod transport;

pub use executor::McpExecutor;
pub use protocol::*;
pub use registry::McpRegistry;
