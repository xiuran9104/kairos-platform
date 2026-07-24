//! HPM-08: Stage Critic — evaluates coverage and success criteria.
pub fn evaluate_stage(completed_actions: u32, total_actions: u32, criteria_met: bool) -> f64 {
    if total_actions == 0 {
        return 0.0;
    }
    let completion = completed_actions as f64 / total_actions as f64;
    if criteria_met {
        (completion + 1.0) / 2.0
    } else {
        completion * 0.5
    }
}
