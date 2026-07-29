//! Toon tooling types — mirror Pent's tool_registry.rs for content production.
use serde::{Deserialize, Serialize};

/// A registered tool in the Toonflow tool catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolDescriptor {
    pub tool_id: String,
    pub version: String,
    pub manifest_version: i32,
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub source: ToonToolSource,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub risk_level: String,
    pub executor: ToonToolExecutorSpec,
    pub resource_profile: ToonToolResourceProfile,
    pub lifecycle_status: ToonToolLifecycleStatus,
    pub health_status: ToonToolHealthStatus,
    /// Searchable document built from name + description + capabilities.
    pub search_document: String,
    pub revision: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToonToolSource {
    Builtin,
    Http,
    Container,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolExecutorSpec {
    pub kind: ToonToolSource,
    pub image: Option<String>,
    pub image_digest: Option<String>,
    pub endpoint: Option<String>,
    pub handler: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolResourceProfile {
    pub cpu_cores: f32,
    pub memory_mb: u64,
    pub timeout_seconds: u64,
    pub max_concurrency: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToonToolLifecycleStatus {
    Active,
    Inactive,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToonToolHealthStatus {
    Ready,
    Degraded,
    Unavailable,
}

/// A routing request from an agent selecting tools by capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolRouteRequest {
    pub request_id: String,
    pub tenant_id: i64,
    pub project_id: Option<i64>,
    pub agent_type: String,
    pub intent: String,
    pub required_capabilities: Vec<String>,
    pub allowed_namespaces: Vec<String>,
    pub completed_tools: Vec<String>,
    pub max_risk: String,
    pub top_k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolRouteCandidate {
    pub tool_id: String,
    pub version: String,
    pub namespace: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolRouteDecision {
    pub request_id: String,
    pub selected: Vec<ToonToolRouteCandidate>,
    pub rejected_count: usize,
    pub registry_revision: i64,
    pub strategy: String,
}

/// An invocation of a specific tool version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolInvocation {
    pub invocation_id: String,
    pub agent_type: String,
    pub tool_id: String,
    pub tool_version: String,
    pub arguments: serde_json::Value,
    pub run_id: Option<i64>,
}

/// Unified execution result from any backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonToolExecutionResult {
    pub invocation_id: String,
    pub tool_id: String,
    pub tool_version: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub content: serde_json::Value,
    pub content_type: String,
    pub duration_ms: u64,
    pub error_message: Option<String>,
}
