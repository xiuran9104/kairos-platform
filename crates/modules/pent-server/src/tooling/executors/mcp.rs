//! MCP executor — delegates tool execution to external MCP servers
//! via the McpRegistry. Replaces the previous stub.
use crate::tooling::mcp::protocol::ContentBlock;
use crate::tooling::ports::ToolExecutionBackend;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus, ToolInvocation,
};
use std::sync::Arc;

use crate::tooling::mcp::registry::McpRegistry;

pub struct McpExecutor {
    pub registry: Arc<McpRegistry>,
}

impl McpExecutor {
    pub fn new(registry: Arc<McpRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ToolExecutionBackend for McpExecutor {
    async fn execute(
        &self,
        desc: &ToolDescriptor,
        inv: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        let start = std::time::Instant::now();
        let server_id = desc
            .executor
            .server_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("MCP 工具缺少 server_id"))?;

        // Strip "mcp_" prefix if present to get the actual MCP tool name
        let mcp_tool_name = desc.tool_id.strip_prefix("mcp_").unwrap_or(&desc.tool_id);

        match self
            .registry
            .call_tool(server_id, mcp_tool_name, inv.arguments.clone())
            .await
        {
            Ok(result) => {
                let duration_ms = start.elapsed().as_millis() as i64;
                let text = result
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.clone()),
                        ContentBlock::Image { mime_type, .. } => {
                            Some(format!("[image: {mime_type}]"))
                        }
                        ContentBlock::Resource { uri, .. } => {
                            Some(format!("[resource: {uri}]"))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let status = if result.is_error {
                    ToolExecutionStatus::Failed
                } else {
                    ToolExecutionStatus::Succeeded
                };

                Ok(ToolExecutionResult {
                    schema_version: 1,
                    tool_name: desc.tool_id.clone(),
                    target: inv.target.clone(),
                    status,
                    summary: text.chars().take(500).collect(),
                    observations: vec![],
                    findings: vec![],
                    artifacts: vec![],
                    raw_output_ref: None,
                    metrics: ToolExecutionMetrics {
                        exit_code: if result.is_error { -1 } else { 0 },
                        duration_ms: duration_ms as u64,
                    },
                    warnings: if result.is_error {
                        vec!["MCP 工具返回错误".into()]
                    } else {
                        vec![]
                    },
                    occurred_at: Utc::now(),
                })
            }
            Err(error) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ToolExecutionResult {
                    schema_version: 1,
                    tool_name: desc.tool_id.clone(),
                    target: inv.target.clone(),
                    status: ToolExecutionStatus::Failed,
                    summary: format!("MCP 调用失败: {error}"),
                    observations: vec![],
                    findings: vec![],
                    artifacts: vec![],
                    raw_output_ref: None,
                    metrics: ToolExecutionMetrics {
                        exit_code: -1,
                        duration_ms,
                    },
                    warnings: vec![format!("MCP 调用失败: {error}")],
                    occurred_at: Utc::now(),
                })
            }
        }
    }
}
