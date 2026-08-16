//! Policy-aware tool candidate routing.

mod filter;
mod scorer;

use chrono::Utc;
use kairos_platform_pent_api::{
    ToolDescriptor, ToolRouteCandidate, ToolRouteDecision, ToolRouteRequest,
};

pub fn route_tools(
    request: &ToolRouteRequest,
    tools: &[ToolDescriptor],
    registry_revision: i64,
    policy_version: i32,
) -> ToolRouteDecision {
    let eligible: Vec<_> = tools
        .iter()
        .filter(|tool| filter::is_eligible(request, tool))
        .collect();
    let rejected_count = tools.len().saturating_sub(eligible.len());
    let mut selected: Vec<ToolRouteCandidate> = eligible
        .into_iter()
        .map(|tool| scorer::score(request, tool))
        .collect();
    selected.sort_by(|left, right| right.score.total_cmp(&left.score));
    // No artificial cap — the caller decides how many tools the model sees.
    if request.top_k > 0 {
        selected.truncate(request.top_k);
    }

    ToolRouteDecision {
        request_id: uuid::Uuid::new_v4().to_string(),
        selected,
        rejected_count,
        policy_version,
        registry_revision,
        strategy: "hard_filter+hybrid_retrieval+deterministic_score".into(),
        decided_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_platform_pent_api::{
        ToolExecutorSpec, ToolHealthStatus, ToolLifecycleStatus, ToolQualityMetrics,
        ToolResourceProfile, ToolSource,
    };

    #[test]
    fn excludes_unhealthy_and_out_of_scope_tools_before_scoring() {
        let request = request();
        let mut healthy = descriptor("nuclei", "scan", "medium");
        healthy.capabilities = vec!["web_vulnerability_scan".into()];
        let mut unavailable = descriptor("afrog", "scan", "medium");
        unavailable.health_status = ToolHealthStatus::Unavailable;
        let dangerous = descriptor("shell", "exec", "critical");
        let decision = route_tools(&request, &[healthy, unavailable, dangerous], 7, 1);
        assert_eq!(decision.selected.len(), 1);
        assert_eq!(decision.selected[0].tool_id, "nuclei");
        assert_eq!(decision.rejected_count, 2);
    }

    fn request() -> ToolRouteRequest {
        ToolRouteRequest {
            run_id: "run".into(),
            tenant_id: 1,
            project_id: None,
            phase: "scan".into(),
            intent: "Web 漏洞扫描".into(),
            target: "https://example.test".into(),
            target_type: "url".into(),
            required_capabilities: vec!["web_vulnerability_scan".into()],
            allowed_namespaces: vec!["scan".into()],
            completed_tools: vec![],
            max_risk: "medium".into(),
            budget_remaining: serde_json::json!({}),
            context: serde_json::json!({}),
            top_k: 5,
        }
    }

    fn descriptor(id: &str, namespace: &str, risk: &str) -> ToolDescriptor {
        ToolDescriptor {
            tool_id: id.into(),
            version: "1".into(),
            manifest_version: 1,
            namespace: namespace.into(),
            name: id.into(),
            description: id.into(),
            capabilities: vec![],
            tags: vec![],
            source: ToolSource::Builtin,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            risk_level: risk.into(),
            approval_policy: "conditional".into(),
            supported_target_types: vec!["url".into()],
            executor: ToolExecutorSpec {
                kind: ToolSource::Builtin,
                endpoint: None,
                server_id: None,
                handler: None,
                execution_class: None,
            },
            resource_profile: ToolResourceProfile {
                cpu_cores: 1.0,
                memory_mb: 512,
                timeout_seconds: 300,
                max_concurrency: 2,
                network_policy: "scope_only".into(),
            },
            health_status: ToolHealthStatus::Ready,
            lifecycle_status: ToolLifecycleStatus::Active,
            embedding_document: id.into(),
            embedding_model: None,
            embedding_version: None,
            vector_record_id: None,
            quality: ToolQualityMetrics::default(),
            revision: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
pub mod retrieval;
