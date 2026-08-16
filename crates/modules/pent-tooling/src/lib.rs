//! Tool contracts and MCP protocol types shared by runtime adapters.
//!
//! Tool implementations, transports and process execution intentionally live
//! outside this crate.

#![forbid(unsafe_code)]

pub mod mcp;
pub mod ports;

/// Canonical tool DTOs remain defined by `pent-api`; this facade gives tool
/// routers and executors a tooling-focused import path.
pub mod contract {
    pub use kairos_platform_pent_api::{
        ToolDescriptor, ToolExecutionResult, ToolInvocation, ToolRouteRequest,
    };
}
