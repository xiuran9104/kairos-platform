use kairos_platform_pent_api::{
    ToolDescriptor, ToolHealthStatus, ToolLifecycleStatus, ToolRouteRequest,
};

pub(super) fn is_eligible(request: &ToolRouteRequest, tool: &ToolDescriptor) -> bool {
    tool.lifecycle_status == ToolLifecycleStatus::Active
        && matches!(
            tool.health_status,
            ToolHealthStatus::Ready | ToolHealthStatus::Degraded
        )
        && (request.allowed_namespaces.is_empty()
            || request.allowed_namespaces.contains(&tool.namespace))
        && (tool.supported_target_types.is_empty()
            || tool.supported_target_types.contains(&request.target_type))
        && request
            .required_capabilities
            .iter()
            .all(|capability| tool.capabilities.contains(capability))
        && !request.completed_tools.contains(&tool.tool_id)
        && risk_rank(&tool.risk_level) <= risk_rank(&request.max_risk)
}

fn risk_rank(value: &str) -> u8 {
    match value {
        "info" | "low" => 1,
        "medium" => 2,
        "high" => 3,
        "critical" => 4,
        _ => u8::MAX,
    }
}
