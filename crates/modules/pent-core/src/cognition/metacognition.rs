use chrono::Utc;
use kairos_platform_pent_api::{
    MetaCognitiveSnapshot, MetaCognitiveState, MetaIntervention, MetaMetrics,
};

pub fn evaluate_meta_state(
    run_id: &str,
    plan_id: &str,
    plan_version: i64,
    metrics: MetaMetrics,
) -> MetaCognitiveSnapshot {
    let (state, intervention, reasons) = if metrics.scope_risk >= 0.5 {
        (
            MetaCognitiveState::SafetyCritical,
            MetaIntervention::Stop,
            vec!["SCOPE_RISK".into()],
        )
    } else if metrics.budget_pressure >= 0.9 {
        (
            MetaCognitiveState::BudgetConstrained,
            MetaIntervention::Pause,
            vec!["BUDGET_PRESSURE".into()],
        )
    } else if metrics.failure_streak >= 3 || metrics.loop_similarity >= 0.85 {
        (
            MetaCognitiveState::Stuck,
            MetaIntervention::Replan,
            vec!["EXECUTION_LOOP".into()],
        )
    } else if metrics.model_disagreement >= 0.7 {
        (
            MetaCognitiveState::Conflicted,
            MetaIntervention::AskUser,
            vec!["MODEL_DISAGREEMENT".into()],
        )
    } else if metrics.plan_confidence < 0.5 || metrics.observation_confidence < 0.5 {
        (
            MetaCognitiveState::Uncertain,
            MetaIntervention::GatherInformation,
            vec!["LOW_CONFIDENCE".into()],
        )
    } else {
        (
            MetaCognitiveState::Confident,
            MetaIntervention::Continue,
            vec![],
        )
    };
    MetaCognitiveSnapshot {
        snapshot_id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.into(),
        plan_id: plan_id.into(),
        plan_version,
        state,
        intervention,
        metrics,
        reason_codes: reasons,
        created_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_risk_stops_before_other_interventions() {
        let snapshot = evaluate_meta_state(
            "run",
            "plan",
            1,
            MetaMetrics {
                plan_confidence: 1.0,
                observation_confidence: 1.0,
                evidence_coverage: 0.0,
                recent_information_gain: 0.0,
                loop_similarity: 0.0,
                failure_streak: 0,
                budget_pressure: 0.0,
                scope_risk: 0.8,
                model_disagreement: 0.0,
            },
        );
        assert_eq!(snapshot.intervention, MetaIntervention::Stop);
    }
}
