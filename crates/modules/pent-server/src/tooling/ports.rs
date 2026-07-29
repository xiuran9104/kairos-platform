use async_trait::async_trait;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutionResult, ToolInvocation, ToolRouteRequest,
};

#[async_trait]
pub trait ToolCatalog: Send + Sync {
    async fn active_tools(
        &self,
        request: &ToolRouteRequest,
    ) -> anyhow::Result<Vec<ToolDescriptor>>;
    async fn get(&self, tool_id: &str, version: &str) -> anyhow::Result<Option<ToolDescriptor>>;
    async fn registry_revision(&self) -> anyhow::Result<i64>;
}

#[async_trait]
pub trait ToolExecutionBackend: Send + Sync {
    async fn execute(
        &self,
        descriptor: &ToolDescriptor,
        invocation: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult>;
}
