use std::sync::Arc;

use kairos_platform_pent_api::{
    ActionReflection, MetaIntervention, PreFlectDirective, PreFlection, ToolExecutionResult,
};

use super::{
    ActionReflector, CognitionEventSink, CognitiveExecutor, DynamicReplanner, MetaMetricsSource,
    PlanRepository, PreReflector, StepPlanner,
};

pub struct CognitionCycleResult {
    pub preflection: Option<PreFlection>,
    pub execution_result: Option<ToolExecutionResult>,
    pub reflection: Option<ActionReflection>,
    pub intervention: MetaIntervention,
    pub plan_version: i64,
}

pub struct CognitionRuntime {
    plans: Arc<dyn PlanRepository>,
    step_planner: Arc<dyn StepPlanner>,
    preflector: Arc<dyn PreReflector>,
    executor: Arc<dyn CognitiveExecutor>,
    reflector: Arc<dyn ActionReflector>,
    replanner: Arc<dyn DynamicReplanner>,
    metrics: Arc<dyn MetaMetricsSource>,
    events: Arc<dyn CognitionEventSink>,
}

impl CognitionRuntime {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plans: Arc<dyn PlanRepository>,
        step_planner: Arc<dyn StepPlanner>,
        preflector: Arc<dyn PreReflector>,
        executor: Arc<dyn CognitiveExecutor>,
        reflector: Arc<dyn ActionReflector>,
        replanner: Arc<dyn DynamicReplanner>,
        metrics: Arc<dyn MetaMetricsSource>,
        events: Arc<dyn CognitionEventSink>,
    ) -> Self {
        Self {
            plans,
            step_planner,
            preflector,
            executor,
            reflector,
            replanner,
            metrics,
            events,
        }
    }

    pub async fn run_cycle(&self, run_id: &str) -> anyhow::Result<CognitionCycleResult> {
        let mut graph = self.plans.current(run_id).await?;
        let metrics = self.metrics.collect(run_id, &graph).await?;
        let meta = kairos_platform_pent_core::cognition::evaluate_meta_state(
            run_id,
            &graph.plan_id,
            graph.version,
            metrics,
        );
        self.plans.record_meta(&meta).await?;
        if meta.intervention != MetaIntervention::Continue {
            self.events
                .emit("metacognition.intervention", serde_json::to_value(&meta)?)
                .await?;
            return Ok(CognitionCycleResult {
                preflection: None,
                execution_result: None,
                reflection: None,
                intervention: meta.intervention,
                plan_version: graph.version,
            });
        }

        let proposal = self.step_planner.propose(&graph).await?;
        self.events
            .emit("plan.action.proposed", serde_json::to_value(&proposal)?)
            .await?;
        let preflection = self.preflector.evaluate(&graph, &proposal).await?;
        self.plans.record_preflection(&preflection).await?;
        if preflection.directive != PreFlectDirective::Execute {
            return Ok(CognitionCycleResult {
                preflection: Some(preflection),
                execution_result: None,
                reflection: None,
                intervention: MetaIntervention::Replan,
                plan_version: graph.version,
            });
        }

        let result = self.executor.execute(&proposal).await?;
        let reflection = self.reflector.reflect(&graph, &proposal, &result).await?;
        self.plans.record_action_reflection(&reflection).await?;
        if let Some(delta) = self
            .replanner
            .replan(&graph, &proposal, &result, &reflection, &meta)
            .await?
        {
            graph = self.plans.apply_delta(&graph, &delta).await?;
            self.events
                .emit("plan.delta.applied", serde_json::to_value(&delta)?)
                .await?;
        }
        Ok(CognitionCycleResult {
            preflection: Some(preflection),
            execution_result: Some(result),
            reflection: Some(reflection),
            intervention: MetaIntervention::Continue,
            plan_version: graph.version,
        })
    }
}
