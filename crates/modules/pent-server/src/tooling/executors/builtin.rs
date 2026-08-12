//! Builtin executor — dispatches to local tool implementations.
use crate::tooling::builtin::local_tools;
use crate::tooling::ports::ToolExecutionBackend;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus, ToolInvocation,
};
use tracing::warn;

pub struct BuiltinExecutor;

#[async_trait]
impl ToolExecutionBackend for BuiltinExecutor {
    async fn execute(
        &self,
        desc: &ToolDescriptor,
        inv: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        let args = inv.arguments.clone();
        let start = std::time::Instant::now();

        let result = match desc.tool_id.as_str() {
            "read" => local_tools::execute_read(args).await,
            "grep" => local_tools::execute_grep(args).await,
            "bash" => local_tools::execute_bash(args).await,
            "web_search" => local_tools::execute_web_search(args).await,
            other => {
                warn!(tool = %other, "builtin tool not implemented");
                Err(anyhow::anyhow!("Builtin 工具 '{}' 尚未实现", other))
            }
        };

        let duration_ms = start.elapsed().as_millis() as i64;
        match result {
            Ok(value) => Ok(ToolExecutionResult {
                schema_version: 1,
                tool_name: desc.tool_id.clone(),
                target: inv.target.clone(),
                status: ToolExecutionStatus::Succeeded,
                summary: serde_json::to_string(&value).unwrap_or_else(|_| "ok".into()),
                observations: vec![],
                findings: vec![],
                artifacts: vec![],
                raw_output_ref: None,
                metrics: ToolExecutionMetrics {
                    exit_code: 0,
                    duration_ms: duration_ms as u64,
                },
                warnings: vec![],
                occurred_at: Utc::now(),
            }),
            Err(error) => Ok(ToolExecutionResult {
                schema_version: 1,
                tool_name: desc.tool_id.clone(),
                target: inv.target.clone(),
                status: ToolExecutionStatus::Failed,
                summary: format!("{error}"),
                observations: vec![],
                findings: vec![],
                artifacts: vec![],
                raw_output_ref: None,
                metrics: ToolExecutionMetrics {
                    exit_code: -1,
                    duration_ms: duration_ms as u64,
                },
                warnings: vec![format!("{error}")],
                occurred_at: Utc::now(),
            }),
        }
    }
}
