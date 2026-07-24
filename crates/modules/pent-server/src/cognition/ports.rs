use async_trait::async_trait;
use kairos_platform_pent_api::{
    ActionProposal, ActionReflection, MetaCognitiveSnapshot, MetaMetrics, PlanDelta, PreFlection,
    TaskCoordinationGraph, ToolExecutionResult,
};

#[async_trait]
pub trait PlanRepository: Send + Sync {
    async fn current(&self, run_id: &str) -> anyhow::Result<TaskCoordinationGraph>;
    async fn save_initial(&self, graph: &TaskCoordinationGraph) -> anyhow::Result<()>;
    async fn apply_delta(
        &self,
        current: &TaskCoordinationGraph,
        delta: &PlanDelta,
    ) -> anyhow::Result<TaskCoordinationGraph>;
    async fn record_meta(&self, snapshot: &MetaCognitiveSnapshot) -> anyhow::Result<()>;
    async fn record_preflection(&self, reflection: &PreFlection) -> anyhow::Result<()>;
    async fn record_action_reflection(&self, reflection: &ActionReflection) -> anyhow::Result<()>;
}

#[async_trait]
pub trait StepPlanner: Send + Sync {
    async fn propose(&self, graph: &TaskCoordinationGraph) -> anyhow::Result<ActionProposal>;
}

#[async_trait]
pub trait PreReflector: Send + Sync {
    async fn evaluate(
        &self,
        graph: &TaskCoordinationGraph,
        proposal: &ActionProposal,
    ) -> anyhow::Result<PreFlection>;
}

#[async_trait]
pub trait CognitiveExecutor: Send + Sync {
    async fn execute(&self, proposal: &ActionProposal) -> anyhow::Result<ToolExecutionResult>;
}

#[async_trait]
pub trait ActionReflector: Send + Sync {
    async fn reflect(
        &self,
        graph: &TaskCoordinationGraph,
        proposal: &ActionProposal,
        result: &ToolExecutionResult,
    ) -> anyhow::Result<ActionReflection>;
}

#[async_trait]
pub trait DynamicReplanner: Send + Sync {
    async fn replan(
        &self,
        graph: &TaskCoordinationGraph,
        proposal: &ActionProposal,
        result: &ToolExecutionResult,
        reflection: &ActionReflection,
        meta: &MetaCognitiveSnapshot,
    ) -> anyhow::Result<Option<PlanDelta>>;
}

#[async_trait]
pub trait MetaMetricsSource: Send + Sync {
    async fn collect(
        &self,
        run_id: &str,
        graph: &TaskCoordinationGraph,
    ) -> anyhow::Result<MetaMetrics>;
}

#[async_trait]
pub trait CognitionEventSink: Send + Sync {
    async fn emit(&self, event_type: &str, payload: serde_json::Value) -> anyhow::Result<()>;
}
