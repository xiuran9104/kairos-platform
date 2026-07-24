use crate::tooling::ports::ToolExecutionBackend;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus, ToolInvocation,
};

pub struct BuiltinExecutor;
#[async_trait]
impl ToolExecutionBackend for BuiltinExecutor {
    async fn execute(
        &self,
        desc: &ToolDescriptor,
        inv: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        Ok(ToolExecutionResult {
            schema_version: 1,
            tool_name: desc.tool_id.clone(),
            target: inv.target.clone(),
            status: ToolExecutionStatus::Succeeded,
            summary: format!("builtin {} completed", desc.tool_id),
            observations: vec![],
            findings: vec![],
            artifacts: vec![],
            raw_output_ref: None,
            metrics: ToolExecutionMetrics {
                exit_code: 0,
                duration_ms: 10,
            },
            warnings: vec![],
            occurred_at: Utc::now(),
        })
    }
}
