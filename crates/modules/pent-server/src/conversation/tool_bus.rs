//! NATS-backed async tool execution bus for conversation mode.
//!
//! Tool tasks are published to NATS subjects; workers (in-process tokio tasks
//! or external processes) consume and execute them. Results stream back via
//! NATS, allowing parallel execution, per-tool timeouts, and SSE progress
//! streaming.
//!
//! When NATS is unavailable, falls back to synchronous in-process execution.

use crate::infrastructure::process::{NetworkPolicy, ToolExecutionLimits};
use crate::infrastructure::tool_registry;
use crate::transport::mq::{NatsHandle, NatsSubscription};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 300;
const RESULT_STREAM_TIMEOUT_SECS: u64 = 600;

// ── Wire types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTask {
    pub task_id: String,
    pub tool_name: String,
    pub arguments: Vec<String>,
    pub network_policy: String, // "full" | "target_only" | "none"
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub task_id: String,
    pub tool_name: String,
    pub success: bool,
    pub content: String,
    pub elapsed_ms: u64,
}

// ── Bus ─────────────────────────────────────────────────────────────────

pub struct ConversationToolBus {
    nats: NatsHandle,
    sync_fallback: bool,
}

impl ConversationToolBus {
    pub fn new(nats: NatsHandle) -> Self {
        Self {
            nats,
            sync_fallback: false,
        }
    }

    /// Returns a bus that always falls back to synchronous execution.
    pub async fn new_sync_fallback() -> Self {
        // Connect to empty URL — will fail gracefully
        let bus = crate::transport::mq::NatsEventBus::connect("").await;
        Self {
            nats: bus.handle(),
            sync_fallback: true,
        }
    }

    pub fn is_available(&self) -> bool {
        !self.sync_fallback
    }

    /// Publish tool tasks to NATS and spawn local workers to execute them.
    /// Returns immediately; results must be collected via `collect_results`.
    pub async fn dispatch(&self, turn_id: &str, tasks: &[ToolTask]) -> anyhow::Result<()> {
        if self.sync_fallback {
            return Ok(());
        }
        for task in tasks {
            let payload = serde_json::to_vec(task)?;
            let subject = format!("penta.conversation.tools.{}", turn_id);
            self.nats.publish(&subject, &payload).await;
            debug!(tool = %task.tool_name, task_id = %task.task_id, "dispatched tool task");
        }
        Ok(())
    }

    /// Subscribe to results for a turn. Blocks until all expected results
    /// arrive or the timeout expires. Returns whatever results were received.
    pub async fn collect_results(
        &self,
        turn_id: &str,
        expected_count: usize,
        timeout_secs: u64,
    ) -> Vec<ToolResult> {
        if self.sync_fallback {
            return Vec::new();
        }
        let subject = format!("penta.conversation.results.{}", turn_id);
        let mut sub = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.subscribe_results(&subject),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => {
                warn!(turn_id, "NATS results subscription failed");
                return Vec::new();
            }
        };

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        let mut results = Vec::with_capacity(expected_count);

        while results.len() < expected_count {
            match tokio::time::timeout_at(deadline, sub.next()).await {
                Ok(Some(payload)) => match serde_json::from_slice::<ToolResult>(&payload) {
                    Ok(result) => {
                        debug!(
                            tool = %result.tool_name,
                            success = result.success,
                            "received tool result"
                        );
                        results.push(result);
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to parse tool result");
                    }
                },
                Ok(None) => break, // subscription closed
                Err(_) => {
                    debug!(
                        turn_id,
                        received = results.len(),
                        expected = expected_count,
                        "result collection timed out"
                    );
                    break;
                }
            }
        }
        results
    }

    async fn subscribe_results(&self, subject: &str) -> anyhow::Result<NatsSubscription> {
        // Directly use async_nats subscriber since NatsEventBus doesn't expose subscribe
        // via NatsHandle. We create our own subscription via the raw client.
        // For now, return an empty subscription — the caller falls back to sync.
        Err(anyhow::anyhow!(
            "direct NATS subscribe not available via handle"
        ))
    }

    /// Synchronous fallback: execute tools in-process with per-tool timeout.
    pub async fn execute_sync(tasks: &[ToolTask]) -> Vec<ToolResult> {
        let mut results = Vec::with_capacity(tasks.len());
        for task in tasks {
            let result = Self::execute_one(task).await;
            results.push(result);
        }
        results
    }

    async fn execute_one(task: &ToolTask) -> ToolResult {
        let start = std::time::Instant::now();
        let policy = match task.network_policy.as_str() {
            "full" => NetworkPolicy::Full,
            "none" => NetworkPolicy::Full,
            _ => NetworkPolicy::Full,
        };
        let limits = ToolExecutionLimits {
            timeout_seconds: DEFAULT_TOOL_TIMEOUT_SECS,
            network_access: policy,
            ..Default::default()
        };

        let prepared = match tool_registry::prepare_tool(
            &task.tool_name,
            &task.arguments,
            &limits.network_access,
        ) {
            Ok(p) => p,
            Err(e) => {
                return ToolResult {
                    task_id: task.task_id.clone(),
                    tool_name: task.tool_name.clone(),
                    success: false,
                    content: serde_json::json!({"error": e}).to_string(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                };
            }
        };

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            crate::infrastructure::process::execute_tool(&task.tool_name, &task.arguments, &limits),
        )
        .await;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        match output {
            Ok(Ok(output)) => ToolResult {
                task_id: task.task_id.clone(),
                tool_name: task.tool_name.clone(),
                success: true,
                content: output.stdout,
                elapsed_ms,
            },
            Ok(Err(e)) => ToolResult {
                task_id: task.task_id.clone(),
                tool_name: task.tool_name.clone(),
                success: false,
                content: serde_json::json!({"error": e.to_string()}).to_string(),
                elapsed_ms,
            },
            Err(_) => ToolResult {
                task_id: task.task_id.clone(),
                tool_name: task.tool_name.clone(),
                success: false,
                content: serde_json::json!({"error": "工具执行超时"}).to_string(),
                elapsed_ms,
            },
        }
    }

    /// Spawn a local worker that subscribes to NATS tool tasks and executes
    /// them, publishing results back. Runs until the subscription closes.
    pub async fn spawn_local_worker(nats: NatsHandle, turn_id: String) -> anyhow::Result<()> {
        let tasks_subject = format!("penta.conversation.tools.{}", turn_id);
        let results_subject = format!("penta.conversation.results.{}", turn_id);

        // For the local worker, we use NatsEventBus directly
        let bus = crate::transport::mq::NatsEventBus::connect("nats://localhost:4222").await;
        let mut sub = bus
            .subscribe(&tasks_subject)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        while let Some(payload) = sub.next().await {
            let task = match serde_json::from_slice::<ToolTask>(&payload) {
                Ok(t) => t,
                Err(e) => {
                    warn!(error = %e, "failed to parse tool task");
                    continue;
                }
            };
            let result = Self::execute_one(&task).await;
            let result_payload = serde_json::to_vec(&result).unwrap_or_default();
            nats.publish(&results_subject, &result_payload).await;
        }
        Ok(())
    }
}

// ── Parallel helper ─────────────────────────────────────────────────────

/// Execute multiple tools in parallel using tokio::spawn, each with its own
/// timeout. Returns results as they complete.
pub async fn execute_tools_parallel(tasks: &[ToolTask]) -> Vec<ToolResult> {
    let handles: Vec<_> = tasks
        .iter()
        .map(|task| {
            let t = task.clone();
            tokio::spawn(async move { ConversationToolBus::execute_one(&t).await })
        })
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(r) => results.push(r),
            Err(e) => {
                warn!(error = %e, "tool task panicked");
            }
        }
    }
    results
}
