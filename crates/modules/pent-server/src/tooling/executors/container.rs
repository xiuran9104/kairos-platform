use crate::infrastructure::process::{ContainerLimits, execute_tool};
use crate::tooling::ports::ToolExecutionBackend;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus, ToolInvocation,
};

pub struct ContainerExecutor;
#[async_trait]
impl ToolExecutionBackend for ContainerExecutor {
    async fn execute(
        &self,
        desc: &ToolDescriptor,
        inv: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        let limits = ContainerLimits::default();
        let args: Vec<String> = inv
            .arguments
            .as_object()
            .map(|o| {
                o.values()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let result = execute_tool(&desc.tool_id, &args, &limits)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let status = if result.exit_code == 0 {
            ToolExecutionStatus::Succeeded
        } else {
            ToolExecutionStatus::Failed
        };
        Ok(ToolExecutionResult {
            schema_version: 1,
            tool_name: desc.tool_id.clone(),
            target: inv.target.clone(),
            status,
            summary: result.stdout.chars().take(200).collect(),
            observations: vec![],
            findings: vec![],
            artifacts: vec![],
            raw_output_ref: None,
            metrics: ToolExecutionMetrics {
                exit_code: result.exit_code as i32,
                duration_ms: result.duration_ms,
            },
            warnings: if !result.stderr.is_empty() {
                vec![result.stderr.chars().take(500).collect()]
            } else {
                vec![]
            },
            occurred_at: Utc::now(),
        })
    }
}
