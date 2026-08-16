//! Runtime-neutral building blocks for penetration-testing agents.
//!
//! This crate owns agent loop state and lifecycle protocol. Transport,
//! persistence, model providers and tool execution remain adapter concerns.

#![forbid(unsafe_code)]

pub mod agent_loop;
