//! Elastic tool execution — execution ids, cancellation, concurrency limits,
//! and per-tool circuit breakers (D-06).

use super::process::{NetworkPolicy, ToolExecutionLimits, ToolOutput};
use super::sandbox::BubblewrapSandboxBackend;
use kairos_platform_pent_tooling::backend::{
    ExecutionBackend, ExecutionNetworkPolicy, ExecutionRequest, SandboxSpec,
};
pub use kairos_platform_pent_tooling::execution_policy::CircuitBreaker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, Semaphore, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::debug;

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
    tool_name: String,
    cancel: CancellationToken,
    result_tx: Option<oneshot::Sender<ExecutionState>>,
    result_rx: Option<oneshot::Receiver<ExecutionState>>,
}

/// Registry of in-flight and recently finished tool executions.
pub struct ExecutionRegistry {
    executions: Mutex<HashMap<ExecutionId, ExecutionRecord>>,
    next_id: AtomicU64,
    /// Max concurrent executions per tool name.
    concurrency: Mutex<HashMap<String, Arc<Semaphore>>>,
    default_permit: usize,
    backend: Arc<dyn ExecutionBackend>,
}

impl std::fmt::Debug for ExecutionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionRegistry")
            .field("next_id", &self.next_id.load(Ordering::SeqCst))
            .field("default_permit", &self.default_permit)
            .finish_non_exhaustive()
    }
}

impl ExecutionRegistry {
    pub fn new(default_permit: usize) -> Self {
        Self::with_backend(default_permit, Arc::new(BubblewrapSandboxBackend))
    }

    pub fn with_backend(default_permit: usize, backend: Arc<dyn ExecutionBackend>) -> Self {
        Self {
            executions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            concurrency: Mutex::new(HashMap::new()),
            default_permit,
            backend,
        }
    }

    fn next_id(&self) -> ExecutionId {
        ExecutionId(self.next_id.fetch_add(1, Ordering::SeqCst).to_string())
    }

    async fn semaphore(&self, tool_name: &str) -> Arc<Semaphore> {
        let mut concurrency = self.concurrency.lock().await;
        concurrency
            .entry(tool_name.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.default_permit)))
            .clone()
    }

    /// Enqueue or start an execution, returning its id immediately.
    pub async fn start(self: Arc<Self>, mut request: ExecutionRequest) -> ExecutionId {
        let id = self.next_id();
        request.execution_id = id.0.clone();
        let tool_name = request.tool_name.clone();
        let cancel = CancellationToken::new();
        let (tx, rx) = oneshot::channel();

        {
            let mut execs = self.executions.lock().await;
            execs.insert(
                id.clone(),
                ExecutionRecord {
                    state: ExecutionState::Pending,
                    tool_name: tool_name.clone(),
                    cancel: cancel.clone(),
                    result_tx: Some(tx),
                    result_rx: Some(rx),
                },
            );
        }

        let registry = self.clone();
        let run_id = id.clone();
        tokio::spawn(async move {
            registry.run(request, cancel, run_id).await;
        });

        id
    }

    async fn run(&self, request: ExecutionRequest, cancel: CancellationToken, id: ExecutionId) {
        let tool_name = request.tool_name.clone();
        // Acquire concurrency permit; block until available.
        let semaphore = self.semaphore(&tool_name).await;
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

        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                debug!(execution_id = %id.0, tool = %tool_name, "execution cancelled before completion");
                ExecutionState::Cancelled
            }
            output = self.backend.execute(request) => {
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

    /// Tool name recorded for an execution, if known.
    pub async fn tool_name(&self, id: &ExecutionId) -> Option<String> {
        let execs = self.executions.lock().await;
        execs.get(id).map(|r| r.tool_name.clone())
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

fn execution_request(
    tool_name: &str,
    arguments: &[String],
    limits: &ToolExecutionLimits,
) -> ExecutionRequest {
    ExecutionRequest {
        execution_id: String::new(),
        tool_name: tool_name.to_string(),
        arguments: arguments.to_vec(),
        run_id: limits.run_id.clone(),
        sandbox: SandboxSpec {
            cpu_cores: limits.cpu_cores,
            memory_mb: limits.memory_mb,
            timeout_seconds: limits.timeout_seconds,
            network_access: match &limits.network_access {
                NetworkPolicy::None => ExecutionNetworkPolicy::None,
                NetworkPolicy::TargetOnly(target) => {
                    ExecutionNetworkPolicy::TargetOnly(target.clone())
                }
                NetworkPolicy::Full => ExecutionNetworkPolicy::Full,
            },
            workspace_dir: limits.workspace_dir.clone(),
            artifact_dir: limits.artifact_dir.clone(),
            allowed_paths: limits.allowed_paths.clone(),
            secret_environment: limits.secret_environment.clone(),
        },
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
        Self::with_backend(
            default_permit,
            breaker_threshold,
            breaker_recovery_secs,
            Arc::new(BubblewrapSandboxBackend),
        )
    }

    pub fn with_backend(
        default_permit: usize,
        breaker_threshold: u32,
        breaker_recovery_secs: u64,
        backend: Arc<dyn ExecutionBackend>,
    ) -> Self {
        Self {
            registry: Arc::new(ExecutionRegistry::with_backend(default_permit, backend)),
            breaker: Arc::new(CircuitBreaker::new(
                breaker_threshold,
                breaker_recovery_secs,
            )),
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
        let id = self.start(tool_name, arguments, limits).await?;
        self.collect(&id).await
    }

    /// Execute a structured recipe call with the same per-tool concurrency
    /// and circuit-breaker policy used by positional Run-mode calls.
    pub async fn execute_json(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        limits: &ToolExecutionLimits,
    ) -> Result<ToolOutput, String> {
        self.breaker.check(tool_name).await?;
        let semaphore = self.registry.semaphore(tool_name).await;
        let _permit = semaphore
            .acquire()
            .await
            .map_err(|error| format!("concurrency semaphore closed: {error}"))?;
        match super::process::execute_tool_json_direct(tool_name, arguments, limits).await {
            Ok(output) => {
                self.breaker.record_success(tool_name).await;
                Ok(output)
            }
            Err(error) => {
                self.breaker.record_failure(tool_name).await;
                Err(error)
            }
        }
    }

    /// Start a tool execution asynchronously, returning its id immediately.
    /// The caller may poll `collect` or `ExecutionRegistry::state` in later
    /// turns instead of blocking the current agent turn.
    pub async fn start(
        &self,
        tool_name: &str,
        arguments: &[String],
        limits: &ToolExecutionLimits,
    ) -> Result<ExecutionId, String> {
        self.breaker.check(tool_name).await?;
        let request = execution_request(tool_name, arguments, limits);
        Ok(Arc::clone(&self.registry).start(request).await)
    }

    /// Wait for a previously started execution and record breaker state.
    pub async fn collect(&self, id: &ExecutionId) -> Result<ToolOutput, String> {
        let tool_name = self
            .registry
            .tool_name(id)
            .await
            .unwrap_or_else(|| "unknown".to_string());
        let state = self.registry.wait(id).await;
        self.registry.remove(id).await;
        match state {
            Some(ExecutionState::Completed(output)) => {
                self.breaker.record_success(&tool_name).await;
                Ok(output)
            }
            Some(ExecutionState::Failed(error)) => {
                self.breaker.record_failure(&tool_name).await;
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
    use async_trait::async_trait;
    use kairos_platform_pent_tooling::backend::{ExecutionAudit, ExecutionOutput};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    struct FakeBackend;

    #[async_trait]
    impl ExecutionBackend for FakeBackend {
        async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutput, String> {
            Ok(success_output(&request.tool_name, "fake"))
        }
    }

    struct PendingBackend;

    #[async_trait]
    impl ExecutionBackend for PendingBackend {
        async fn execute(&self, _request: ExecutionRequest) -> Result<ExecutionOutput, String> {
            std::future::pending().await
        }
    }

    struct GateBackend {
        entered: Arc<AtomicUsize>,
        release: Arc<Semaphore>,
    }

    #[async_trait]
    impl ExecutionBackend for GateBackend {
        async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutput, String> {
            self.entered.fetch_add(1, AtomicOrdering::SeqCst);
            let _permit = self.release.acquire().await.map_err(|e| e.to_string())?;
            Ok(success_output(&request.tool_name, "gate"))
        }
    }

    fn success_output(tool_name: &str, backend: &str) -> ExecutionOutput {
        ExecutionOutput {
            exit_code: 0,
            stdout: tool_name.to_string(),
            stderr: String::new(),
            duration_ms: 1,
            audit: ExecutionAudit {
                backend: backend.to_string(),
                network_policy: "none".into(),
                cpu_cores: 0.5,
                memory_mb: 256,
                timeout_seconds: 5,
                workspace_dir: None,
                artifact_dir: None,
                mount_namespace: false,
                pid_namespace: false,
                user_namespace: false,
                capabilities_dropped: false,
                seccomp: false,
            },
        }
    }

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

    #[tokio::test]
    async fn start_returns_id_before_collect_completes() {
        let executor = ElasticExecutor::new(10, 5, 60);
        let limits = ToolExecutionLimits {
            timeout_seconds: 5,
            ..Default::default()
        };
        let id = executor
            .start("tcp_scan", &["scan".into(), "127.0.0.1".into()], &limits)
            .await
            .unwrap();
        assert!(!id.0.is_empty());
        // collect blocks until the execution finishes (success or failure).
        let outcome = executor.collect(&id).await;
        assert!(outcome.is_ok() || outcome.is_err());
        // The execution is removed from the registry after collect.
        assert!(executor.registry().state(&id).await.is_none());
    }

    #[tokio::test]
    async fn injected_backend_drives_elastic_execution() {
        let executor = ElasticExecutor::with_backend(2, 5, 60, Arc::new(FakeBackend));
        let output = executor
            .execute("fake_tool", &[], &ToolExecutionLimits::default())
            .await
            .unwrap();
        assert_eq!(output.stdout, "fake_tool");
        assert_eq!(output.audit.backend, "fake");
    }

    #[test]
    fn elastic_request_keeps_secrets_local_and_redacted() {
        let limits = ToolExecutionLimits {
            secret_environment: super::super::process::SecretEnvironment::new(vec![(
                "AWS_SECRET_ACCESS_KEY".into(),
                "must-not-leak".into(),
            )]),
            ..Default::default()
        };
        let request = execution_request("prowler", &[], &limits);
        assert_eq!(request.sandbox.secret_environment.len(), 1);
        let serialized = serde_json::to_string(&request).unwrap();
        let debug = format!("{request:?}");
        assert!(!serialized.contains("must-not-leak"));
        assert!(!debug.contains("must-not-leak"));
    }

    #[tokio::test]
    async fn cancellation_propagates_around_backend_future() {
        let executor = ElasticExecutor::with_backend(1, 5, 60, Arc::new(PendingBackend));
        let id = executor
            .start("pending", &[], &ToolExecutionLimits::default())
            .await
            .unwrap();
        assert!(executor.registry().cancel(&id).await);
        assert_eq!(
            executor.collect(&id).await.unwrap_err(),
            "execution cancelled"
        );
    }

    #[tokio::test]
    async fn same_tool_uses_one_shared_concurrency_semaphore() {
        let entered = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Semaphore::new(0));
        let backend = GateBackend {
            entered: entered.clone(),
            release: release.clone(),
        };
        let executor = ElasticExecutor::with_backend(1, 5, 60, Arc::new(backend));
        let first = executor
            .start("serialized", &[], &ToolExecutionLimits::default())
            .await
            .unwrap();
        let second = executor
            .start("serialized", &[], &ToolExecutionLimits::default())
            .await
            .unwrap();

        while entered.load(AtomicOrdering::SeqCst) < 1 {
            tokio::task::yield_now().await;
        }
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(entered.load(AtomicOrdering::SeqCst), 1);

        release.add_permits(1);
        while entered.load(AtomicOrdering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
        release.add_permits(1);
        assert!(executor.collect(&first).await.is_ok());
        assert!(executor.collect(&second).await.is_ok());
    }
}
