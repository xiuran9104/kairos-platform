use kairos_platform_pent_api::cognition::*;
pub struct DynamicReplanner;
impl DynamicReplanner {
    pub fn evaluate(
        failures: u32,
        gain: f64,
        budget: f64,
        high_sev: bool,
        steer: Option<&str>,
    ) -> Option<ReplanLevel> {
        if failures >= 3 {
            return Some(ReplanLevel::Stage);
        }
        if gain < 0.1 && failures >= 2 {
            return Some(ReplanLevel::Tool);
        }
        if budget > 0.8 {
            return Some(ReplanLevel::Parameter);
        }
        if high_sev {
            return Some(ReplanLevel::Stage);
        }
        if steer.is_some() {
            return Some(ReplanLevel::Global);
        }
        None
    }
    pub fn build_delta(plan_id: &str, base: i64, level: ReplanLevel, reason: &str) -> PlanDelta {
        PlanDelta {
            delta_id: uuid::Uuid::new_v4().to_string(),
            plan_id: plan_id.into(),
            base_version: base,
            replan_level: level,
            reason_codes: vec![reason.into()],
            operations: vec![],
            created_at: chrono::Utc::now(),
            proposed_by: String::new(),
        }
    }
}
