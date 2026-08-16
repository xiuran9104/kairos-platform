//! Sandbox execution contracts shared by the runtime and the local bubblewrap adapter.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "mode", content = "target")]
pub enum ExecutionNetworkPolicy {
    None,
    TargetOnly(String),
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxSpec {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    pub network_access: ExecutionNetworkPolicy,
    /// Optional persistent host directory mounted read-write at `/workspace`.
    /// The infrastructure adapter creates a run-scoped directory when omitted.
    #[serde(default)]
    pub workspace_dir: Option<String>,
    /// Optional persistent host directory mounted read-write at `/artifacts`.
    /// The infrastructure adapter creates a run-scoped directory when omitted.
    #[serde(default)]
    pub artifact_dir: Option<String>,
    /// Explicit host paths made visible inside the otherwise minimal mount namespace.
    /// They are mounted read-write at their absolute path; the first directory is also
    /// exposed at `/state` for stateful HTTP tools.
    #[serde(default)]
    pub allowed_paths: Vec<String>,
}

impl Default for SandboxSpec {
    fn default() -> Self {
        Self {
            cpu_cores: 0.5,
            memory_mb: 256,
            timeout_seconds: 300,
            network_access: ExecutionNetworkPolicy::None,
            workspace_dir: None,
            artifact_dir: None,
            allowed_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionRequest {
    pub execution_id: String,
    pub tool_name: String,
    pub arguments: Vec<String>,
    pub run_id: Option<String>,
    pub sandbox: SandboxSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionAudit {
    pub backend: String,
    pub network_policy: String,
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub workspace_dir: Option<String>,
    #[serde(default)]
    pub artifact_dir: Option<String>,
    #[serde(default)]
    pub mount_namespace: bool,
    #[serde(default)]
    pub pid_namespace: bool,
    #[serde(default)]
    pub user_namespace: bool,
    #[serde(default)]
    pub capabilities_dropped: bool,
    #[serde(default)]
    pub seccomp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub audit: ExecutionAudit,
}

#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutput, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_defaults_deny_network_access() {
        let spec = SandboxSpec::default();
        assert_eq!(spec.network_access, ExecutionNetworkPolicy::None);
        assert_eq!(spec.timeout_seconds, 300);
    }
}
