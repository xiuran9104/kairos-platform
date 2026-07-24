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
    pub const TOOL_MANAGE: &str = "pent:tool:manage";
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
        matches!(self, ToolRiskLevel::High | ToolRiskLevel::Critical)
    }
}
