//! RMOS — Rule-based Memory and Optimization System.
//!
//! Sub-modules:
//! - `memory`          — Structured pentest state (findings, attack_surface, next_actions)
//! - `knowledge_cache` — Layered wiki search with caching (only new knowledge injected)
//! - `attack_graph`    — Attack chain as directed graph (asset → endpoint → vuln → credential → privilege)
//! - `injector`        — Auto-injects memory + knowledge + graph into conversation system prompt

pub mod attack_graph;
pub mod injector;
pub mod knowledge_cache;
pub mod memory;

// Re-export core types
pub use attack_graph::AttackGraph;
pub use injector::{Injection, build_injected_context, prepare_injection};
pub use knowledge_cache::KnowledgeCache;
pub use memory::PentestMemory;
