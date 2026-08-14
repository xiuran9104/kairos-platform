//! Elastic tool execution — execution ids, cancellation, concurrency limits,
//! and per-tool circuit breakers (D-06).

use super::process::{ToolExecutionLimits, ToolOutput, execute_tool_direct};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use tokio::sync::{Mutex, Semaphore, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Unique identifier for an in-flight tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionId(pub String);

impl ExecutionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for ExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Final state of an elastic execution.
#[derive(Debug, Clone)]
pub enum ExecutionState {
    Pending,
    Running,
    Completed(ToolOutput),
    Cancelled,
    Failed(String),
    Rejected(String),
}

/// Record held for every tracked execution.
struct ExecutionRecord {
    state: ExecutionState,
    cancel: CancellationToken,
    result_tx: Option<oneshot::Sender<ExecutionState>>,
    result_rx: Option<oneshot::Receiver<ExecutionState>>,
}

/// Registry of in-flight and recently finished tool executions.
pub struct ExecutionRegistry {
    executions: Mutex<HashMap<ExecutionId, ExecutionRecord>>,
    next_id: AtomicU64,
    /// Max concurrent executions per tool name.
    concurrency: HashMap<String, Arc<Semaphore>>,
    default_permit: usize,
}

impl std::fmt::Debug for ExecutionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionRegistry")
            .field("next_id", &self.next_id.load(Ordering::SeqCst))
            .field("concurrency_tools", &self.concurrency.len())
            .field("default_permit", &self.default_permit)
            .finish_non_exhaustive()
    }
}

impl ExecutionRegistry {
    pub fn new(default_permit: usize) -> Self {
        Self {
            executions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            concurrency: HashMap::new(),
            default_permit,
        }
    }

    fn next_id(&self) -> ExecutionId {
        ExecutionId(self.next_id.fetch_add(1, Ordering::SeqCst).to_string())
    }

    fn semaphore(&self, tool_name: &str) -> Arc<Semaphore> {
        self.concurrency
            .get(tool_name)
            .cloned()
            .unwrap_or_else(|| Arc::new(Semaphore::new(self.default_permit)))
    }

    /// Enqueue or start an execution, returning its id immediately.
    pub async fn start(
        self: Arc<Self>,
        tool_name: String,
        arguments: Vec<String>,
        limits: ToolExecutionLimits,
    ) -> ExecutionId {
        let id = self.next_id();
        let cancel = CancellationToken::new();
        let (tx, rx) = oneshot::channel();

        {
            let mut execs = self.executions.lock().await;
            execs.insert(
                id.clone(),
                ExecutionRecord {
                    state: ExecutionState::Pending,
                    cancel: cancel.clone(),
                    result_tx: Some(tx),
                    result_rx: Some(rx),
                },
            );
        }

        let registry = self.clone();
        let run_id = id.clone();
        tokio::spawn(async move {
            registry
                .run(tool_name, arguments, limits, cancel, run_id)
                .await;
        });

        id
    }

    async fn run(
        &self,
        tool_name: String,
        arguments: Vec<String>,
        limits: ToolExecutionLimits,
        cancel: CancellationToken,
        id: ExecutionId,
    ) {
        // Acquire concurrency permit; block until available.
        let semaphore = self.semaphore(&tool_name);
        let permit = match semaphore.acquire().await {
            Ok(p) => p,
            Err(error) => {
                self.finish(
                    &id,
                    ExecutionState::Failed(format!("concurrency semaphore closed: {error}")),
                )
                .await;
                return;
            }
        };

        self.set_state(&id, ExecutionState::Running).await;

        let tool_name_for_exec = tool_name.clone();
        let arguments_for_exec = arguments.clone();
        let limits_for_exec = limits.clone();
        let cancel_for_exec = cancel.clone();

        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                debug!(execution_id = %id.0, tool = %tool_name, "execution cancelled before completion");
                ExecutionState::Cancelled
            }
            output = execute_with_cancel(tool_name_for_exec, arguments_for_exec, limits_for_exec, cancel_for_exec) => {
                match output {
                    Ok(tool_output) => ExecutionState::Completed(tool_output),
                    Err(error) => ExecutionState::Failed(error),
                }
            }
        };

        drop(permit);
        self.finish(&id, result).await;
    }

    /// Cancel a running execution by id. Returns false if the id is unknown.
    pub async fn cancel(&self, id: &ExecutionId) -> bool {
        let execs = self.executions.lock().await;
        if let Some(record) = execs.get(id) {
            record.cancel.cancel();
            true
        } else {
            false
        }
    }

    /// Get the current state of an execution.
    pub async fn state(&self, id: &ExecutionId) -> Option<ExecutionState> {
        let execs = self.executions.lock().await;
        execs.get(id).map(|r| r.state.clone())
    }

    /// Wait for an execution to finish and return its final state.
    pub async fn wait(&self, id: &ExecutionId) -> Option<ExecutionState> {
        let rx = {
            let mut execs = self.executions.lock().await;
            execs.get_mut(id).and_then(|r| r.result_rx.take())
        };
        match rx {
            Some(rx) => rx.await.ok(),
            None => self.state(id).await,
        }
    }

    /// Remove a finished execution from the registry.
    pub async fn remove(&self, id: &ExecutionId) {
        let mut execs = self.executions.lock().await;
        execs.remove(id);
    }

    async fn set_state(&self, id: &ExecutionId, state: ExecutionState) {
        let mut execs = self.executions.lock().await;
        if let Some(record) = execs.get_mut(id) {
            record.state = state;
        }
    }

    async fn finish(&self, id: &ExecutionId, state: ExecutionState) {
        let tx = {
            let mut execs = self.executions.lock().await;
            if let Some(record) = execs.get_mut(id) {
                record.state = state.clone();
                record.result_tx.take()
            } else {
                None
            }
        };
        if let Some(tx) = tx {
            let _ = tx.send(state);
        }
    }
}

async fn execute_with_cancel(
    tool_name: String,
    arguments: Vec<String>,
    limits: ToolExecutionLimits,
    cancel: CancellationToken,
) -> Result<ToolOutput, String> {
    // Wrap the synchronous execute_tool in a cancellable task.
    let handle = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err("cancelled".to_string()),
                result = execute_tool_direct(&tool_name, &arguments, &limits) => result,
            }
        })
    });

    match handle.await {
        Ok(result) => result,
        Err(error) => Err(format!("execution task panicked: {error}")),
    }
}

/// Per-tool circuit breaker.
pub struct CircuitBreaker {
    failures: Mutex<HashMap<String, BreakerState>>,
    threshold: u32,
    recovery_secs: u64,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("threshold", &self.threshold)
            .field("recovery_secs", &self.recovery_secs)
            .finish_non_exhaustive()
    }
}

struct BreakerState {
    failures: u32,
    open_until: Option<std::time::Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, recovery_secs: u64) -> Self {
        Self {
            failures: Mutex::new(HashMap::new()),
            threshold,
            recovery_secs,
        }
    }

    /// Returns Err if the breaker is open for this tool.
    pub async fn check(&self, tool_name: &str) -> Result<(), String> {
        let mut states = self.failures.lock().await;
        let state = states.entry(tool_name.to_string()).or_insert(BreakerState {
            failures: 0,
            open_until: None,
        });

        if let Some(open_until) = state.open_until {
            if std::time::Instant::now() < open_until {
                return Err(format!("tool {tool_name} circuit breaker is open"));
            }
            // Recovery window passed — half-close.
            state.open_until = None;
            state.failures = 0;
        }
        Ok(())
    }

    /// Record a success, resetting failures.
    pub async fn record_success(&self, tool_name: &str) {
        let mut states = self.failures.lock().await;
        states.insert(
            tool_name.to_string(),
            BreakerState {
                failures: 0,
                open_until: None,
            },
        );
    }

    /// Record a failure; opens the breaker when threshold is reached.
    pub async fn record_failure(&self, tool_name: &str) {
        let mut states = self.failures.lock().await;
        let state = states.entry(tool_name.to_string()).or_insert(BreakerState {
            failures: 0,
            open_until: None,
        });
        state.failures += 1;
        if state.failures >= self.threshold {
            warn!(tool = %tool_name, threshold = self.threshold, "circuit breaker opened");
            state.open_until = Some(
                std::time::Instant::now() + std::time::Duration::from_secs(self.recovery_secs),
            );
        }
    }
}

/// Elastic executor that combines the execution registry and circuit breaker.
///
/// This is the public handle for D-06 resilient execution. When initialized as
/// the global executor, all calls to [`crate::infrastructure::process::execute_tool`]
/// are routed through it.
#[derive(Debug, Clone)]
pub struct ElasticExecutor {
    registry: Arc<ExecutionRegistry>,
    breaker: Arc<CircuitBreaker>,
}

impl ElasticExecutor {
    pub fn new(default_permit: usize, breaker_threshold: u32, breaker_recovery_secs: u64) -> Self {
        Self {
            registry: Arc::new(ExecutionRegistry::new(default_permit)),
            breaker: Arc::new(CircuitBreaker::new(breaker_threshold, breaker_recovery_secs)),
        }
    }

    pub fn from_env() -> Self {
        let default_permit = std::env::var("PENTA_MAX_CONCURRENT_EXECUTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        let breaker_threshold = std::env::var("PENTA_CIRCUIT_BREAKER_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        let breaker_recovery_secs = std::env::var("PENTA_CIRCUIT_BREAKER_RECOVERY_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        Self::new(default_permit, breaker_threshold, breaker_recovery_secs)
    }

    pub fn registry(&self) -> &ExecutionRegistry {
        &self.registry
    }

    pub fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }

    /// Execute a tool with concurrency limiting, circuit breaker, and cancellation.
    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: &[String],
        limits: &ToolExecutionLimits,
    ) -> Result<ToolOutput, String> {
        self.breaker.check(tool_name).await?;

        let id = Arc::clone(&self.registry)
            .start(tool_name.to_string(), arguments.to_vec(), limits.clone())
            .await;
        let state = self.registry.wait(&id).await;
        self.registry.remove(&id).await;

        match state {
            Some(ExecutionState::Completed(output)) => {
                self.breaker.record_success(tool_name).await;
                Ok(output)
            }
            Some(ExecutionState::Failed(error)) => {
                self.breaker.record_failure(tool_name).await;
                Err(error)
            }
            Some(ExecutionState::Cancelled) => Err("execution cancelled".into()),
            Some(ExecutionState::Rejected(error)) => Err(error),
            Some(ExecutionState::Pending) | Some(ExecutionState::Running) | None => {
                Err("execution finished in unexpected state".into())
            }
        }
    }
}

static GLOBAL_EXECUTOR: OnceLock<Arc<ElasticExecutor>> = OnceLock::new();

/// Initialize the global elastic executor. Must be called once at startup.
pub fn init_global_executor(executor: Arc<ElasticExecutor>) -> Result<(), String> {
    GLOBAL_EXECUTOR
        .set(executor)
        .map_err(|_| "global elastic executor already initialized".to_string())
}

/// Access the global elastic executor, if one has been initialized.
pub fn global_executor() -> Option<Arc<ElasticExecutor>> {
    GLOBAL_EXECUTOR.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_id_is_unique() {
        let a = ExecutionId::new();
        let b = ExecutionId::new();
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn cancel_returns_false_for_unknown_execution() {
        let registry = Arc::new(ExecutionRegistry::new(10));
        let id = ExecutionId::new();
        assert!(!registry.cancel(&id).await);
    }

    #[tokio::test]
    async fn circuit_opens_after_threshold() {
        let breaker = CircuitBreaker::new(2, 60);
        assert!(breaker.check("nmap").await.is_ok());
        breaker.record_failure("nmap").await;
        breaker.record_failure("nmap").await;
        assert!(breaker.check("nmap").await.is_err());
        breaker.record_success("nmap").await;
        assert!(breaker.check("nmap").await.is_ok());
    }

    #[tokio::test]
    async fn circuit_tracks_tools_independently() {
        let breaker = CircuitBreaker::new(2, 60);
        breaker.record_failure("nmap").await;
        breaker.record_failure("nmap").await;
        assert!(breaker.check("nmap").await.is_err());
        assert!(breaker.check("nuclei").await.is_ok());
    }
}
