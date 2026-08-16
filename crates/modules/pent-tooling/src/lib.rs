//! Tool contracts and MCP protocol types shared by runtime adapters.
//!
//! Reusable tool implementations and transports live here; deployment-specific
//! persistence and process sandbox backends remain in server adapters.

#![forbid(unsafe_code)]

pub mod backend;
pub mod catalog;
pub mod dispatcher;
pub mod execution_policy;
pub mod executor;
pub mod manifest;
pub mod mcp;
pub mod ports;

/// Canonical tool DTOs remain defined by `pent-api`; this facade gives tool
/// routers and executors a tooling-focused import path.
pub mod contract {
    pub use kairos_platform_pent_api::{
        ToolDescriptor, ToolExecutionResult, ToolInvocation, ToolRouteRequest,
    };
}
