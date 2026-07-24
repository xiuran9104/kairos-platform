use std::sync::Arc;

use kairos_platform_pent_api::{
    PolicyEffect, ToolExecutionResult, ToolInvocation, ToolRouteDecision, ToolRouteRequest,
};

use super::{ToolCatalog, ToolExecutionBackend, ToolPolicyGate, ToolTelemetry};

pub struct ToolOrchestrator {
    catalog: Arc<dyn ToolCatalog>,
    policy: Arc<dyn ToolPolicyGate>,
    executor: Arc<dyn ToolExecutionBackend>,
    telemetry: Arc<dyn ToolTelemetry>,
}

impl ToolOrchestrator {
    pub fn new(
        catalog: Arc<dyn ToolCatalog>,
        policy: Arc<dyn ToolPolicyGate>,
        executor: Arc<dyn ToolExecutionBackend>,
        telemetry: Arc<dyn ToolTelemetry>,
    ) -> Self {
        Self {
            catalog,
            policy,
            executor,
            telemetry,
        }
    }

    pub async fn route(&self, request: &ToolRouteRequest) -> anyhow::Result<ToolRouteDecision> {
        let tools = self.catalog.active_tools(request).await?;
        let revision = self.catalog.registry_revision().await?;
        let decision =
            kairos_platform_pent_core::tool_router::route_tools(request, &tools, revision, 1);
        self.telemetry.route_decided(request, &decision).await?;
        Ok(decision)
    }

    pub async fn execute(
        &self,
        invocation: &ToolInvocation,
    ) -> anyhow::Result<ToolExecutionResult> {
        let descriptor = self
            .catalog
            .get(&invocation.tool_id, &invocation.tool_version)
            .await?
            .ok_or_else(|| anyhow::anyhow!("工具或版本不存在"))?;
        let decision = self.policy.evaluate(&descriptor, invocation).await?;
        match decision.effect {
            PolicyEffect::Allow => {}
            PolicyEffect::Ask => anyhow::bail!("工具调用需要人工审批"),
            PolicyEffect::Deny => anyhow::bail!("工具调用被策略拒绝"),
        }
        let result = self.executor.execute(&descriptor, invocation).await?;
        self.telemetry
            .invocation_completed(invocation, &result)
            .await?;
        Ok(result)
    }
}
