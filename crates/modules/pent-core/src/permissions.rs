//! Execution-time permission and approval state machine.

use chrono::Utc;
use kairos_platform_pent_api::models::{ApprovalStatus, PentApproval};
use kairos_platform_pent_api::permissions::ToolRiskLevel;

/// Create an approval request for a high-risk tool.
pub fn create_approval(
    run_id: &str,
    tool_call_id: &str,
    tool_name: &str,
    risk_level: ToolRiskLevel,
    description: &str,
    ttl_minutes: i64,
) -> PentApproval {
    PentApproval {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        tool_call_id: tool_call_id.to_string(),
        risk_level: format!("{:?}", risk_level).to_lowercase(),
        tool_name: tool_name.to_string(),
        description: description.to_string(),
        status: ApprovalStatus::Pending,
        requested_by: String::new(),
        decided_by: None,
        decision: None,
        expires_at: Utc::now() + chrono::Duration::minutes(ttl_minutes),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Apply a decision to an approval.
pub fn decide_approval(approval: &mut PentApproval, decision: &str, decided_by: &str) {
    approval.status = match decision {
        "approved" => ApprovalStatus::Approved,
        "denied" => ApprovalStatus::Denied,
        _ => ApprovalStatus::Denied,
    };
    approval.decision = Some(decision.to_string());
    approval.decided_by = Some(decided_by.to_string());
    approval.updated_at = Utc::now();
}

/// Check if approval has expired.
pub fn is_approval_expired(approval: &PentApproval) -> bool {
    approval.status == ApprovalStatus::Pending && approval.expires_at < Utc::now()
}
