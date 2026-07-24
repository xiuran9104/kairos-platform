//! HPM-08: Reflector Aggregator — merges multi-level reflections.
pub fn aggregate(action_score: f64, stage_score: f64, run_score: f64) -> f64 {
    // Stage score dominates, safety-critical items cannot be averaged away
    if action_score < 0.3 || run_score < 0.3 {
        return action_score.min(run_score);
    }
    action_score * 0.3 + stage_score * 0.4 + run_score * 0.3
}
