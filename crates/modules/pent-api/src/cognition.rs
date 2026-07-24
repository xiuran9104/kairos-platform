//! H-PER-M planning, reflection, and metacognitive contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanNodeType {
    Stage,
    Task,
    Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanNodeStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
    Blocked,
    Stale,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanEdgeType {
    Requires,
    Conditional,
    Produces,
    Invalidates,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanBudget {
    pub max_actions: u32,
    pub max_tool_calls: u32,
    pub max_duration_seconds: u64,
    pub max_tokens: u64,
    pub max_cost: f64,
    pub max_concurrency: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanNode {
    pub node_id: String,
    pub node_type: PlanNodeType,
    pub parent_id: Option<String>,
    pub namespace: String,
    pub objective: String,
    pub capability: Option<String>,
    pub target: Option<String>,
    pub status: PlanNodeStatus,
    pub priority: i32,
    pub success_criteria: serde_json::Value,
    pub budget: PlanBudget,
    pub attempt: u32,
    pub max_attempts: u32,
    pub observation_refs: Vec<String>,
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanEdge {
    pub from: String,
    pub to: String,
    pub edge_type: PlanEdgeType,
    pub condition: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCoordinationGraph {
    pub plan_id: String,
    pub run_id: String,
    pub version: i64,
    pub objective: String,
    pub nodes: Vec<PlanNode>,
    pub edges: Vec<PlanEdge>,
    pub global_budget: PlanBudget,
    pub memory_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum PlanDeltaOperation {
    AddNode {
        node: PlanNode,
    },
    UpdateNode {
        node_id: String,
        patch: serde_json::Value,
    },
    RemoveNode {
        node_id: String,
    },
    AddEdge {
        edge: PlanEdge,
    },
    RemoveEdge {
        from: String,
        to: String,
    },
    MarkStale {
        node_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanDelta {
    pub delta_id: String,
    pub plan_id: String,
    pub base_version: i64,
    pub replan_level: ReplanLevel,
    pub reason_codes: Vec<String>,
    pub operations: Vec<PlanDeltaOperation>,
    pub proposed_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplanLevel {
    Parameter,
    Tool,
    Stage,
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionProposal {
    pub action_id: String,
    pub plan_id: String,
    pub plan_version: i64,
    pub node_id: String,
    pub capability: String,
    pub target: String,
    pub argument_template: serde_json::Value,
    pub expected_outputs: Vec<String>,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreFlectDirective {
    Execute,
    Replace,
    Ask,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreFlection {
    pub reflection_id: String,
    pub action_id: String,
    pub directive: PreFlectDirective,
    pub expected_information_gain: f32,
    pub confidence: f32,
    pub predicted_outcomes: Vec<String>,
    pub risk_codes: Vec<String>,
    pub alternatives: Vec<String>,
    pub requires_approval: bool,
    pub reasoning_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionReflection {
    pub reflection_id: String,
    pub action_id: String,
    pub useful: bool,
    pub information_gain: f32,
    pub tool_selection_quality: f32,
    pub result_quality: f32,
    pub retry: bool,
    pub reason_codes: Vec<String>,
    pub lessons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetaCognitiveState {
    Confident,
    Uncertain,
    Stuck,
    Conflicted,
    BudgetConstrained,
    SafetyCritical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetaMetrics {
    pub plan_confidence: f32,
    pub observation_confidence: f32,
    pub evidence_coverage: f32,
    pub recent_information_gain: f32,
    pub loop_similarity: f32,
    pub failure_streak: u32,
    pub budget_pressure: f32,
    pub scope_risk: f32,
    pub model_disagreement: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetaIntervention {
    Continue,
    GatherInformation,
    Replan,
    AskUser,
    RequestApproval,
    Pause,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetaCognitiveSnapshot {
    pub snapshot_id: String,
    pub run_id: String,
    pub plan_id: String,
    pub plan_version: i64,
    pub state: MetaCognitiveState,
    pub intervention: MetaIntervention,
    pub metrics: MetaMetrics,
    pub reason_codes: Vec<String>,
    pub created_at: DateTime<Utc>,
}
