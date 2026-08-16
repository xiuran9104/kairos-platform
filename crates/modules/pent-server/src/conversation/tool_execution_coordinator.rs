use std::sync::Arc;
use std::time::{Duration, Instant};

use kairos_platform_pent_runtime::ports::{
    ConversationToolExecutor, InlineToolExecutor, ModelToolCall, RuntimeToolCall,
    RuntimeToolResult, ToolExecutionContext,
};
use kairos_platform_pent_runtime::tool_dispatch::{ToolDispatchPlan, ToolExecutionClass};
use kairos_platform_pent_runtime::tool_result::{NormalizedToolResult, normalize_tool_result};
use tokio::sync::Semaphore;

pub struct CoordinatedToolResult {
    pub index: usize,
    pub call_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub result: RuntimeToolResult,
    pub normalized: NormalizedToolResult,
    pub elapsed: Duration,
    pub inline: bool,
}

pub struct ToolExecutionCoordinator {
    external: Arc<dyn ConversationToolExecutor>,
    max_read_parallelism: usize,
}

impl ToolExecutionCoordinator {
    pub fn from_env(external: Arc<dyn ConversationToolExecutor>) -> Self {
        let max_read_parallelism = std::env::var("PENTA_MAX_PARALLEL_READ_TOOLS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(8)
            .clamp(1, 64);
        Self::new(external, max_read_parallelism)
    }

    pub fn new(external: Arc<dyn ConversationToolExecutor>, max_read_parallelism: usize) -> Self {
        Self {
            external,
            max_read_parallelism: max_read_parallelism.max(1),
        }
    }

    pub async fn execute(
        &self,
        calls: &[ModelToolCall],
        plan: &ToolDispatchPlan,
        context: &ToolExecutionContext,
        inline: &dyn InlineToolExecutor,
    ) -> Vec<CoordinatedToolResult> {
        let mut results = Vec::with_capacity(calls.len());
        let mut reads = Vec::new();
        let mut destructive = Vec::new();
        let semaphore = Arc::new(Semaphore::new(self.max_read_parallelism));

        for (index, call) in calls.iter().enumerate() {
            if let Some(reason) = plan.blocked_reason(&call.id) {
                results.push(blocked_result(index, call, reason));
                continue;
            }
            let class = plan
                .decision(&call.id)
                .map(|decision| decision.class)
                .unwrap_or(ToolExecutionClass::Destructive);
            let runtime_call = runtime_call(call);
            if class == ToolExecutionClass::Inline && inline.supports(&call.name) {
                let started = Instant::now();
                let result = inline.execute(&runtime_call, context).await;
                let normalized = normalize_tool_result(&result);
                results.push(CoordinatedToolResult {
                    index,
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    arguments: call.arguments.clone(),
                    result,
                    normalized,
                    elapsed: started.elapsed(),
                    inline: true,
                });
            } else if class == ToolExecutionClass::Destructive {
                destructive.push((index, runtime_call));
            } else {
                let executor = self.external.clone();
                let execution_context = context.clone();
                let permit = semaphore.clone();
                let failed_call = runtime_call.clone();
                let handle = tokio::spawn(async move {
                    let Ok(_permit) = permit.acquire_owned().await else {
                        return failed_external(index, runtime_call, "并发控制器已关闭");
                    };
                    execute_external(executor, index, runtime_call, execution_context).await
                });
                reads.push((index, failed_call, handle));
            }
        }

        for (index, call, handle) in reads {
            match handle.await {
                Ok(result) => results.push(result),
                Err(error) => {
                    tracing::warn!(%error, tool = %call.name, "parallel read tool panicked");
                    results.push(failed_external(index, call, &error.to_string()));
                }
            }
        }

        for (index, call) in destructive {
            let failed_call = call.clone();
            let executor = self.external.clone();
            let execution_context = context.clone();
            let handle = tokio::spawn(async move {
                execute_external(executor, index, call, execution_context).await
            });
            match handle.await {
                Ok(result) => results.push(result),
                Err(error) => {
                    tracing::warn!(%error, tool = %failed_call.name, "destructive tool panicked");
                    results.push(failed_external(index, failed_call, &error.to_string()));
                }
            }
        }

        results.sort_by_key(|result| result.index);
        results
    }
}

fn failed_external(index: usize, call: RuntimeToolCall, error: &str) -> CoordinatedToolResult {
    let result = RuntimeToolResult {
        tool_call_id: call.id.clone(),
        tool_name: call.name.clone(),
        content: serde_json::json!({"error": error, "retryable": false}).to_string(),
        success: false,
    };
    let normalized = normalize_tool_result(&result);
    CoordinatedToolResult {
        index,
        call_id: call.id.clone(),
        tool_name: call.name.clone(),
        arguments: call.arguments,
        result,
        normalized,
        elapsed: Duration::ZERO,
        inline: false,
    }
}

fn runtime_call(call: &ModelToolCall) -> RuntimeToolCall {
    RuntimeToolCall {
        id: call.id.clone(),
        name: call.name.clone(),
        arguments: call.arguments.clone(),
    }
}

async fn execute_external(
    executor: Arc<dyn ConversationToolExecutor>,
    index: usize,
    call: RuntimeToolCall,
    context: ToolExecutionContext,
) -> CoordinatedToolResult {
    let started = Instant::now();
    let result = executor.execute(&call, &context).await;
    let normalized = normalize_tool_result(&result);
    CoordinatedToolResult {
        index,
        call_id: call.id,
        tool_name: call.name,
        arguments: call.arguments,
        result,
        normalized,
        elapsed: started.elapsed(),
        inline: false,
    }
}

fn blocked_result(index: usize, call: &ModelToolCall, reason: &str) -> CoordinatedToolResult {
    let result = RuntimeToolResult {
        tool_call_id: call.id.clone(),
        tool_name: call.name.clone(),
        content: serde_json::json!({
            "error": reason,
            "error_code": "TOOL_BLOCKED",
            "retryable": false,
            "blocked": true,
            "blocked_by": "hitl"
        })
        .to_string(),
        success: false,
    };
    let normalized = normalize_tool_result(&result);
    CoordinatedToolResult {
        index,
        call_id: call.id.clone(),
        tool_name: call.name.clone(),
        arguments: call.arguments.clone(),
        result,
        normalized,
        elapsed: Duration::ZERO,
        inline: true,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use kairos_platform_pent_runtime::tool_dispatch::ToolDispatchDecision;

    struct NoInline;

    #[async_trait::async_trait]
    impl InlineToolExecutor for NoInline {
        fn supports(&self, _tool_name: &str) -> bool {
            false
        }

        async fn execute(
            &self,
            call: &RuntimeToolCall,
            _context: &ToolExecutionContext,
        ) -> RuntimeToolResult {
            RuntimeToolResult {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                content: "{}".into(),
                success: true,
            }
        }
    }

    #[derive(Default)]
    struct TrackingExecutor {
        active_reads: AtomicUsize,
        max_reads: AtomicUsize,
        active_destructive: AtomicUsize,
        max_destructive: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ConversationToolExecutor for TrackingExecutor {
        async fn execute(
            &self,
            call: &RuntimeToolCall,
            _context: &ToolExecutionContext,
        ) -> RuntimeToolResult {
            let (active, maximum) = if call.name.starts_with("destroy") {
                (&self.active_destructive, &self.max_destructive)
            } else {
                (&self.active_reads, &self.max_reads)
            };
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            RuntimeToolResult {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                content: "{}".into(),
                success: true,
            }
        }
    }

    fn call(index: usize, name: &str) -> ModelToolCall {
        ModelToolCall {
            id: format!("call-{index}"),
            name: name.into(),
            arguments: format!(r#"{{"index":{index}}}"#),
            execution_class: ToolExecutionClass::Destructive,
        }
    }

    fn plan(calls: &[ModelToolCall], classes: &[ToolExecutionClass]) -> ToolDispatchPlan {
        ToolDispatchPlan {
            decisions: calls
                .iter()
                .zip(classes)
                .map(|(call, class)| ToolDispatchDecision {
                    tool_call_id: call.id.clone(),
                    class: *class,
                    repair: None,
                    blocked: None,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn limits_reads_serializes_destructive_and_preserves_order() {
        let calls = vec![
            call(0, "read-a"),
            call(1, "read-b"),
            call(2, "read-c"),
            call(3, "destroy-a"),
            call(4, "destroy-b"),
        ];
        let classes = vec![
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::Destructive,
            ToolExecutionClass::Destructive,
        ];
        let external = Arc::new(TrackingExecutor::default());
        let results = ToolExecutionCoordinator::new(external.clone(), 2)
            .execute(
                &calls,
                &plan(&calls, &classes),
                &ToolExecutionContext {
                    scope_snapshot: serde_json::json!({}),
                    session_id: "session-1".into(),
                    run_id: Some("run-1".into()),
                },
                &NoInline,
            )
            .await;

        assert_eq!(
            results
                .iter()
                .map(|result| result.index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(external.max_reads.load(Ordering::SeqCst), 2);
        assert_eq!(external.max_destructive.load(Ordering::SeqCst), 1);
        assert_eq!(results[2].arguments, r#"{"index":2}"#);
    }
}
