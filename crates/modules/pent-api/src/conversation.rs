//! Stable contracts for Pent Agent conversation turns.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{NormalizedFinding, ToolObservation};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMessageType {
    Chat,
    Steer,
    ProvideContext,
    RequestSummary,
    RequestReview,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationTurnStatus {
    Queued,
    Thinking,
    Streaming,
    WaitingTool,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationAttachmentRef {
    pub artifact_id: String,
    pub name: String,
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnRequest {
    pub session_id: String,
    pub content: String,
    pub message_type: ConversationMessageType,
    pub client_request_id: String,
    pub attachments: Vec<ConversationAttachmentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationIntent {
    pub kind: ConversationMessageType,
    pub confidence: f32,
    pub requires_run_instruction: bool,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMessage {
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelUsage {
    pub model_id: Option<String>,
    pub model_role: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationCitation {
    pub kind: String,
    pub reference_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationPlanUpdate {
    pub directive: String,
    pub capabilities: Vec<String>,
    pub target: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationTurnError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnResult {
    pub turn_id: String,
    pub session_id: String,
    pub run_id: Option<String>,
    pub status: ConversationTurnStatus,
    pub message: AgentMessage,
    pub intent: ConversationIntent,
    pub plan_update: Option<ConversationPlanUpdate>,
    pub observations: Vec<ToolObservation>,
    pub findings: Vec<NormalizedFinding>,
    pub citations: Vec<ConversationCitation>,
    pub usage: ModelUsage,
    pub error: Option<ConversationTurnError>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationContext {
    pub context_version: i32,
    pub session_id: String,
    pub run_id: Option<String>,
    pub system_instruction: String,
    pub objective_snapshot: String,
    pub scope_snapshot: serde_json::Value,
    pub policy_snapshot: serde_json::Value,
    pub budget_snapshot: serde_json::Value,
    pub run_summary: String,
    pub recent_messages: Vec<AgentMessage>,
    pub evidence_refs: Vec<String>,
    pub memory_refs: Vec<String>,
    pub tool_ids: Vec<String>,
    pub estimated_tokens: u64,
}
