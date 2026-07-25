//! Pent Agent permission codes and risk levels.

/// Platform RBAC permission codes.
#[allow(dead_code)]
pub mod permission_codes {
    pub const PROJECT_READ: &str = "pent:project:read";
    pub const PROJECT_CREATE: &str = "pent:project:create";
    pub const TASK_CREATE: &str = "pent:task:create";
    pub const TASK_READ: &str = "pent:task:read";
    pub const TASK_CANCEL: &str = "pent:task:cancel";
    pub const RUN_READ: &str = "pent:run:read";
    pub const RUN_EXECUTE: &str = "pent:run:execute";
    pub const APPROVAL_READ: &str = "pent:approval:read";
    pub const APPROVAL_DECIDE: &str = "pent:approval:decide";
    pub const EVIDENCE_READ: &str = "pent:evidence:read";
    pub const ARTIFACT_DOWNLOAD: &str = "pent:artifact:download";
    pub const AGENT_CHAT: &str = "pent:agent:chat";
    pub const DASHBOARD_READ: &str = "pent:dashboard:read";
    pub const FINDING_READ: &str = "pent:finding:read";
    pub const RMOS_READ: &str = "pent:rmos:read";
    pub const RMOS_FEEDBACK: &str = "pent:rmos:feedback";
    pub const SETTINGS_READ: &str = "pent:settings:read";
    pub const SETTINGS_MANAGE: &str = "pent:settings:manage";
    pub const TOOL_READ: &str = "pent:tool:read";
    pub const TOOL_MANAGE: &str = "pent:tool:manage";
    pub const TRAFFIC_READ: &str = "pent:traffic:read";
}

/// Tool execution risk levels for approval gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl ToolRiskLevel {
    pub fn requires_approval(&self) -> bool {
        matches!(
            self,
            ToolRiskLevel::Medium | ToolRiskLevel::High | ToolRiskLevel::Critical
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ToolRiskLevel;

    #[test]
    fn medium_and_higher_risk_requires_approval() {
        assert!(!ToolRiskLevel::Low.requires_approval());
        assert!(ToolRiskLevel::Medium.requires_approval());
        assert!(ToolRiskLevel::High.requires_approval());
        assert!(ToolRiskLevel::Critical.requires_approval());
    }
}
