//! Bubblewrap implementation of the shared local sandbox execution contract.

use async_trait::async_trait;
use kairos_platform_pent_tooling::backend::{
    ExecutionBackend, ExecutionNetworkPolicy, ExecutionOutput, ExecutionRequest,
};

use super::process::{NetworkPolicy, ToolExecutionLimits, execute_tool_direct};

#[derive(Debug, Clone, Copy, Default)]
pub struct BubblewrapSandboxBackend;

#[async_trait]
impl ExecutionBackend for BubblewrapSandboxBackend {
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutput, String> {
        let limits = ToolExecutionLimits {
            cpu_cores: request.sandbox.cpu_cores,
            memory_mb: request.sandbox.memory_mb,
            timeout_seconds: request.sandbox.timeout_seconds,
            network_access: match request.sandbox.network_access {
                ExecutionNetworkPolicy::None => NetworkPolicy::None,
                ExecutionNetworkPolicy::TargetOnly(target) => NetworkPolicy::TargetOnly(target),
                ExecutionNetworkPolicy::Full => NetworkPolicy::Full,
            },
            workspace_dir: request.sandbox.workspace_dir,
            artifact_dir: request.sandbox.artifact_dir,
            allowed_paths: request.sandbox.allowed_paths,
            run_id: request.run_id,
        };
        execute_tool_direct(&request.tool_name, &request.arguments, &limits).await
    }
}
