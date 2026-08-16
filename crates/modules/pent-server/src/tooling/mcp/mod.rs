//! MCP (Model Context Protocol) client infrastructure.
//! Manages connections to external MCP servers via JSON-RPC over stdio,
//! HTTP, or SSE, discovers their tools, and executes tool calls.

pub mod internal;
pub mod client;
/// Compatibility facade for MCP clients using the historical path.
pub mod protocol {
    pub use kairos_platform_pent_tooling::mcp::*;
}
pub mod recipes;
pub mod registry;
pub mod transport;
