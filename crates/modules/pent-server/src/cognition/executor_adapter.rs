use chrono::Utc;
use kairos_platform_pent_api::cognition::ActionProposal;
use kairos_platform_pent_api::{ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus};
pub struct ExecutorAdapter;
impl ExecutorAdapter {
    pub async fn execute(&self, p: &ActionProposal) -> anyhow::Result<ToolExecutionResult> {
        Ok(ToolExecutionResult {
            schema_version: 1,
            tool_name: p.capability.clone(),
            target: p.target.clone(),
            status: ToolExecutionStatus::Succeeded,
            summary: format!("{} done", p.capability),
            observations: vec![],
            findings: vec![],
            artifacts: vec![],
            raw_output_ref: None,
            metrics: ToolExecutionMetrics {
                exit_code: 0,
                duration_ms: 100,
            },
            warnings: vec![],
            occurred_at: Utc::now(),
        })
    }
}
