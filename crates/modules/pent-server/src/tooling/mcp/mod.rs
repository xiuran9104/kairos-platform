//! MCP (Model Context Protocol) client infrastructure.
//! Manages connections to external MCP servers via JSON-RPC over stdio,
//! HTTP, or SSE, discovers their tools, and executes tool calls.

pub mod client;
pub mod protocol;
pub mod registry;
pub mod transport;
