use kairos_platform_pent_api::{ToolDescriptor, ToolRouteCandidate, ToolRouteRequest};

pub(super) fn score(request: &ToolRouteRequest, tool: &ToolDescriptor) -> ToolRouteCandidate {
    let capability_score = if request.required_capabilities.is_empty() {
        0.5
    } else {
        1.0
    };
    let intent = request.intent.to_lowercase();
    let keyword_matches = tool
        .tags
        .iter()
        .chain(std::iter::once(&tool.name))
        .filter(|value| intent.contains(&value.to_lowercase()))
        .count() as f64;
    let lexical_score = (keyword_matches / 3.0).min(1.0);
    let quality_score =
        tool.quality.execution_success_rate * 0.5 + tool.quality.useful_result_rate * 0.5;
    let health_score = match tool.health_status {
        kairos_platform_pent_api::ToolHealthStatus::Ready => 1.0,
        kairos_platform_pent_api::ToolHealthStatus::Degraded => 0.5,
        _ => 0.0,
    };
    let score =
        capability_score * 0.4 + lexical_score * 0.2 + quality_score * 0.25 + health_score * 0.15;

    ToolRouteCandidate {
        tool_id: tool.tool_id.clone(),
        version: tool.version.clone(),
        namespace: tool.namespace.clone(),
        score,
        reasons: vec![
            "通过 Scope、风险、健康和目标类型硬过滤".into(),
            format!("能力匹配分 {capability_score:.2}"),
            format!("历史质量分 {quality_score:.2}"),
        ],
        requires_approval: matches!(tool.risk_level.as_str(), "high" | "critical")
            || tool.approval_policy == "always",
    }
}
