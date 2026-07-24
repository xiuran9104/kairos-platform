use kairos_platform_pent_api::cognition::*;
pub struct MetaMetricsSource;
impl MetaMetricsSource {
    pub fn evaluate(
        confidence: f64,
        failures: u32,
        budget: f64,
        scope: f64,
        loop_sim: f64,
    ) -> MetaIntervention {
        if scope > 0.7 {
            MetaIntervention::Stop
        } else if failures >= 3 {
            MetaIntervention::Replan
        } else if budget > 0.9 {
            MetaIntervention::Pause
        } else if loop_sim > 0.8 {
            MetaIntervention::Replan
        } else if confidence < 0.4 {
            MetaIntervention::AskUser
        } else {
            MetaIntervention::Continue
        }
    }
    pub fn snapshot(
        run_id: &str,
        plan_id: &str,
        ver: i64,
        confidence: f64,
        failures: u32,
        budget: f64,
        scope: f64,
    ) -> MetaCognitiveSnapshot {
        MetaCognitiveSnapshot {
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            run_id: run_id.into(),
            plan_id: plan_id.into(),
            plan_version: ver,
            state: MetaCognitiveState::Confident,
            intervention: Self::evaluate(confidence, failures, budget, scope, 0.0),
            metrics: MetaMetrics {
                plan_confidence: confidence as f32,
                observation_confidence: 0.5,
                evidence_coverage: 0.0,
                recent_information_gain: 0.3,
                loop_similarity: 0.0,
                failure_streak: failures,
                budget_pressure: budget as f32,
                scope_risk: scope as f32,
                model_disagreement: 0.0,
            },
            reason_codes: vec![],
            created_at: chrono::Utc::now(),
        }
    }
}
