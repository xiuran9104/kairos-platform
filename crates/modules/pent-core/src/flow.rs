//! Flow and subtask state machine.

use kairos_platform_pent_api::models::{FlowStatus, PentFlow, PentSubtask, SubtaskStatus};

/// Transition a flow to a new status.
pub fn transition_flow(flow: &mut PentFlow, new_status: FlowStatus) {
    flow.status = new_status;
    flow.updated_at = chrono::Utc::now();
}

/// Transition a subtask to a new status.
pub fn transition_subtask(subtask: &mut PentSubtask, new_status: SubtaskStatus) {
    subtask.status = new_status;
    subtask.updated_at = chrono::Utc::now();
}

/// Check if all subtasks are terminal (Completed, Failed, Skipped).
pub fn all_subtasks_terminal(subtasks: &[PentSubtask]) -> bool {
    subtasks.iter().all(|s| {
        matches!(
            s.status,
            SubtaskStatus::Completed | SubtaskStatus::Failed | SubtaskStatus::Skipped
        )
    })
}

/// Find the next pending subtask by sequence order.
pub fn next_pending_subtask(subtasks: &[PentSubtask]) -> Option<&PentSubtask> {
    subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Pending)
        .min_by_key(|s| s.sequence)
}
