//! Run and Agent event contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Structured event emitted during a Run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentEvent {
    pub event_id: String,
    pub run_id: String,
    pub sequence: i64,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub payload: serde_json::Value,
    pub schema_version: i32,
}

impl PentEvent {
    pub fn new(
        run_id: String,
        sequence: i64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            run_id,
            sequence,
            event_type: event_type.to_string(),
            occurred_at: Utc::now(),
            payload,
            schema_version: 1,
        }
    }
}

/// Well-known event type constants.
#[allow(dead_code)]
pub mod event_types {
    pub const RUN_CREATED: &str = "run.created";
    pub const RUN_STARTED: &str = "run.started";
    pub const RUN_CANCEL_REQUESTED: &str = "run.cancel_requested";
    pub const RUN_CANCELLED: &str = "run.cancelled";
    pub const RUN_COMPLETED: &str = "run.completed";
    pub const RUN_FAILED: &str = "run.failed";
    pub const FLOW_PLANNED: &str = "flow.planned";
    pub const PLAN_HIERARCHY_CREATED: &str = "plan.hierarchy.created";
    pub const PLAN_ACTION_PROPOSED: &str = "plan.action.proposed";
    pub const PREFLECT_STARTED: &str = "preflect.started";
    pub const PREFLECT_COMPLETED: &str = "preflect.completed";
    pub const PREFLECT_BLOCKED: &str = "preflect.blocked";
    pub const ACTION_OBSERVED: &str = "action.observed";
    pub const REPLAN_TRIGGERED: &str = "replan.triggered";
    pub const PLAN_DELTA_APPLIED: &str = "plan.delta.applied";
    pub const REFLECTION_ACTION_COMPLETED: &str = "reflection.action.completed";
    pub const REFLECTION_STAGE_COMPLETED: &str = "reflection.stage.completed";
    pub const REFLECTION_RUN_COMPLETED: &str = "reflection.run.completed";
    pub const METACOGNITION_STATE_CHANGED: &str = "metacognition.state.changed";
    pub const METACOGNITION_INTERVENTION: &str = "metacognition.intervention";
    pub const LESSON_CREATED: &str = "lesson.created";
    pub const LESSON_PROMOTED: &str = "lesson.promoted";
    pub const PLANNER_FEEDBACK: &str = "planner.feedback";
    pub const CONVERSATION_TURN_CREATED: &str = "conversation.turn.created";
    pub const CONVERSATION_INTENT_DETECTED: &str = "conversation.intent.detected";
    pub const CONVERSATION_CONTEXT_READY: &str = "conversation.context.ready";
    pub const CONVERSATION_MESSAGE_DELTA: &str = "conversation.message.delta";
    pub const CONVERSATION_MESSAGE_COMPLETED: &str = "conversation.message.completed";
    pub const CONVERSATION_TURN_FAILED: &str = "conversation.turn.failed";
    pub const CONVERSATION_TURN_CANCELLED: &str = "conversation.turn.cancelled";
    pub const CONVERSATION_SUMMARY_UPDATED: &str = "conversation.summary.updated";
    pub const CONVERSATION_INSTRUCTION_CREATED: &str = "conversation.instruction.created";
    pub const MODEL_REQUEST_STARTED: &str = "model.request.started";
    pub const MODEL_REQUEST_COMPLETED: &str = "model.request.completed";
    pub const MODEL_REQUEST_FAILED: &str = "model.request.failed";
    pub const SUBTASK_STARTED: &str = "subtask.started";
    pub const SUBTASK_COMPLETED: &str = "subtask.completed";
    pub const SUBTASK_FAILED: &str = "subtask.failed";
    pub const AGENT_MESSAGE_DELTA: &str = "agent.message.delta";
    pub const AGENT_MESSAGE_COMPLETED: &str = "agent.message.completed";
    pub const TOOL_REQUESTED: &str = "tool.requested";
    pub const TOOL_STARTED: &str = "tool.started";
    pub const TOOL_COMPLETED: &str = "tool.completed";
    pub const TOOL_FAILED: &str = "tool.failed";
    pub const TOOL_RESULT_NORMALIZED: &str = "tool.result.normalized";
    pub const TOOL_REGISTRY_CHANGED: &str = "tool.registry.changed";
    pub const TOOL_HEALTH_CHANGED: &str = "tool.health.changed";
    pub const TOOL_ROUTE_REQUESTED: &str = "tool.route.requested";
    pub const TOOL_ROUTE_DECIDED: &str = "tool.route.decided";
    pub const TOOL_ROUTE_FALLBACK: &str = "tool.route.fallback";
    pub const TOOL_POLICY_DECIDED: &str = "tool.policy.decided";
    pub const TOOL_INVOCATION_QUEUED: &str = "tool.invocation.queued";
    pub const TOOL_INVOCATION_STARTED: &str = "tool.invocation.started";
    pub const TOOL_FEEDBACK_RECORDED: &str = "tool.feedback.recorded";
    pub const APPROVAL_REQUESTED: &str = "approval.requested";
    pub const APPROVAL_DECIDED: &str = "approval.decided";
    pub const EVIDENCE_CREATED: &str = "evidence.created";
    pub const FINDING_CREATED: &str = "finding.created";
    pub const ARTIFACT_CREATED: &str = "artifact.created";
}
