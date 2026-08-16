//! Runtime-neutral building blocks for penetration-testing agents.
//!
//! This crate owns agent loop state and lifecycle protocol. Transport,
//! persistence, model providers and tool execution remain adapter concerns.

#![forbid(unsafe_code)]

pub mod agent_loop;
pub mod agent_runner;
pub mod agent_scheduler;
pub mod context_compaction;
pub mod context_guard;
pub mod context_lifecycle;
pub mod observation;
pub mod orchestrator;
pub mod ports;
pub mod tool_dispatch;
pub mod tool_execution;
pub mod tool_result;
