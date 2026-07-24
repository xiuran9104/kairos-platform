use crate::tooling::ports::ToolExecutionBackend;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus, ToolInvocation,
};

pub struct McpExecutor {
    pub endpoint: String,
}
#[async_trait]
impl ToolExecutionBackend for McpExecutor {
    async fn execute(
        &self,
        desc: &ToolDescriptor,
        inv: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        Ok(ToolExecutionResult {
            schema_version: 1,
            tool_name: desc.tool_id.clone(),
            target: inv.target.clone(),
            status: ToolExecutionStatus::Failed,
            summary: format!("MCP executor not yet implemented for tool {}", desc.tool_id),
            observations: vec![],
            findings: vec![],
            artifacts: vec![],
            raw_output_ref: None,
            metrics: ToolExecutionMetrics {
                exit_code: -1,
                duration_ms: 0,
            },
            warnings: vec!["MCP executor is a stub — no actual MCP call was made".into()],
            occurred_at: Utc::now(),
        })
    }
}
