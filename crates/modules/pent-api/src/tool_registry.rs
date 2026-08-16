//! Contracts shared by tool registration, routing, policy, and execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Mcp,
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecycleStatus {
    Draft,
    Active,
    Disabled,
    Deprecated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolHealthStatus {
    Unknown,
    Ready,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutorSpec {
    pub kind: ToolSource,
    pub endpoint: Option<String>,
    pub server_id: Option<String>,
    pub handler: Option<String>,
    /// Runtime scheduling hint: inline, read_only, or destructive.
    /// Missing metadata must be handled conservatively by the runtime.
    #[serde(default)]
    pub execution_class: Option<ToolExecutionClass>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionClass {
    Inline,
    ReadOnly,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResourceProfile {
    pub cpu_cores: f32,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    pub max_concurrency: u32,
    pub network_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolQualityMetrics {
    pub execution_success_rate: f64,
    pub useful_result_rate: f64,
    pub finding_confirmation_rate: f64,
    pub false_positive_rate: f64,
    pub average_duration_ms: f64,
    pub sample_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDescriptor {
    pub tool_id: String,
    pub version: String,
    pub manifest_version: i32,
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub source: ToolSource,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub risk_level: String,
    pub approval_policy: String,
    pub supported_target_types: Vec<String>,
    pub executor: ToolExecutorSpec,
    pub resource_profile: ToolResourceProfile,
    pub health_status: ToolHealthStatus,
    pub lifecycle_status: ToolLifecycleStatus,
    pub embedding_document: String,
    pub embedding_model: Option<String>,
    pub embedding_version: Option<i32>,
    pub vector_record_id: Option<String>,
    pub quality: ToolQualityMetrics,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolRouteRequest {
    pub run_id: String,
    pub tenant_id: i64,
    pub project_id: Option<String>,
    pub phase: String,
    pub intent: String,
    pub target: String,
    pub target_type: String,
    pub required_capabilities: Vec<String>,
    pub allowed_namespaces: Vec<String>,
    pub completed_tools: Vec<String>,
    pub max_risk: String,
    pub budget_remaining: serde_json::Value,
    pub context: serde_json::Value,
    pub top_k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolRouteCandidate {
    pub tool_id: String,
    pub version: String,
    pub namespace: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolRouteDecision {
    pub request_id: String,
    pub selected: Vec<ToolRouteCandidate>,
    pub rejected_count: usize,
    pub policy_version: i32,
    pub registry_revision: i64,
    pub strategy: String,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInvocation {
    pub invocation_id: String,
    pub run_id: String,
    pub project_id: Option<String>,
    pub tool_id: String,
    pub tool_version: String,
    pub target: String,
    pub arguments: serde_json::Value,
    pub scope_snapshot: serde_json::Value,
    pub policy_snapshot: serde_json::Value,
    pub route_decision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolPolicyDecision {
    pub effect: PolicyEffect,
    pub policy_version: i32,
    pub reason_codes: Vec<String>,
    pub constraints: serde_json::Value,
}
