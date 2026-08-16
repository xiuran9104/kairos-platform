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
    pub const ROLE_READ: &str = "pent:role:read";
    pub const ROLE_MANAGE: &str = "pent:role:manage";
    pub const MCP_READ: &str = "pent:mcp:read";
    pub const MCP_MANAGE: &str = "pent:mcp:manage";
    pub const SKILL_READ: &str = "pent:skill:read";
    pub const SKILL_MANAGE: &str = "pent:skill:manage";
    pub const WORKFLOW_READ: &str = "pent:workflow:read";
    pub const WORKFLOW_MANAGE: &str = "pent:workflow:manage";
    pub const AGENT_READ: &str = "pent:agent:read";
    pub const AGENT_MANAGE: &str = "pent:agent:manage";
    pub const ASSET_READ: &str = "pent:asset:read";
    pub const ASSET_MANAGE: &str = "pent:asset:manage";
    pub const FACT_READ: &str = "pent:fact:read";
    pub const FACT_MANAGE: &str = "pent:fact:manage";
    pub const FILE_READ: &str = "pent:file:read";
    pub const FILE_MANAGE: &str = "pent:file:manage";
    pub const MONITOR_READ: &str = "pent:monitor:read";
    pub const INFOCOLLECT_READ: &str = "pent:infocollect:read";
    pub const INFOCOLLECT_MANAGE: &str = "pent:infocollect:manage";
    pub const KNOWLEDGE_LOG_READ: &str = "pent:knowledge-log:read";
    pub const NODE_READ: &str = "pent:node:read";
    pub const NODE_MANAGE: &str = "pent:node:manage";
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
