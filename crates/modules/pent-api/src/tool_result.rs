//! Stable, tool-independent execution result contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionStatus {
    Succeeded,
    Partial,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolObservation {
    pub category: String,
    pub key: String,
    pub value: serde_json::Value,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedFinding {
    pub title: String,
    pub severity: String,
    pub description: String,
    pub target: String,
    pub confidence: f32,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolArtifactRef {
    pub artifact_id: String,
    pub kind: String,
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolExecutionMetrics {
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// Canonical result consumed by the planning layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutionResult {
    pub schema_version: i32,
    pub tool_name: String,
    pub target: String,
    pub status: ToolExecutionStatus,
    pub summary: String,
    pub observations: Vec<ToolObservation>,
    pub findings: Vec<NormalizedFinding>,
    pub artifacts: Vec<ToolArtifactRef>,
    pub raw_output_ref: Option<String>,
    pub metrics: ToolExecutionMetrics,
    pub warnings: Vec<String>,
    pub occurred_at: DateTime<Utc>,
}
