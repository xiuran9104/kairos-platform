//! Pent Agent domain models — stable public API contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A security project linking Pent tasks to kairos-platform assets/tenants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentProject {
    pub id: String,
    pub tenant_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub asset_scope: serde_json::Value,
    pub status: ProjectStatus,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

/// A security task (one execution intent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentTask {
    pub id: String,
    pub tenant_id: i64,
    pub project_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub target_scope: serde_json::Value,
    pub tool_profile_id: Option<String>,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// An execution plan (Flow).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentFlow {
    pub id: String,
    pub task_id: String,
    pub plan: serde_json::Value,
    pub status: FlowStatus,
    pub version: i32,
    pub policy_snapshot: ExecutionPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FlowStatus {
    Planned,
    Running,
    Completed,
    Failed,
}

/// A single execution run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentRun {
    pub id: String,
    pub task_id: String,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub tenant_id: i64,
    pub user_id: String,
    pub project_id: Option<String>,
    pub objective_snapshot: Option<String>,
    pub scope_snapshot: serde_json::Value,
    pub policy_snapshot: serde_json::Value,
    pub budget_snapshot: serde_json::Value,
    pub status: RunStatus,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Queued,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub tenant_id: i64,
    pub project_id: Option<String>,
    pub objective: String,
    pub constraints: serde_json::Value,
    pub budget: serde_json::Value,
    pub scope_proposal: serde_json::Value,
    pub confirmed_scope_snapshot: Option<serde_json::Value>,
    pub status: AgentSessionStatus,
    pub active_run_id: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatus {
    Draft,
    Ready,
    Running,
    Paused,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub message_type: String,
    pub content: String,
    pub payload: serde_json::Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInstruction {
    pub id: String,
    pub session_id: Option<String>,
    pub run_id: String,
    pub instruction_type: String,
    pub content: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub rejection_reason: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub applied_at: Option<DateTime<Utc>>,
}

/// Execution policy snapshot — immutable per run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub allowed_targets: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub ask_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub max_concurrency: u32,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub initiator_id: String,
    pub data_scope: String,
    pub approval_rules: Option<serde_json::Value>,
    pub policy_version: i32,
}

/// Subtask within a Flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentSubtask {
    pub id: String,
    pub flow_id: String,
    pub run_id: String,
    pub name: String,
    pub tool_name: String,
    pub params: serde_json::Value,
    pub status: SubtaskStatus,
    pub sequence: i32,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Tool call record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentToolCall {
    pub id: String,
    pub run_id: String,
    pub subtask_id: Option<String>,
    pub tool_name: String,
    pub params_summary: serde_json::Value,
    pub permission_decision: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentApproval {
    pub id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub risk_level: String,
    pub tool_name: String,
    pub description: String,
    pub status: ApprovalStatus,
    pub requested_by: String,
    pub decided_by: Option<String>,
    pub decision: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

/// Evidence record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentEvidence {
    pub id: String,
    pub run_id: String,
    pub tool_call_id: Option<String>,
    pub source: String,
    pub content_hash: String,
    pub object_key: Option<String>,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
}

/// Finding (vulnerability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentFinding {
    pub id: String,
    pub run_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub target: String,
    pub evidence_ids: Vec<String>,
    pub assigned_to: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Artifact stored in MinIO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PentArtifact {
    pub id: String,
    pub run_id: String,
    pub object_key: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
}
