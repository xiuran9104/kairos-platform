//! Command DTOs — create, cancel, approve operations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub asset_scope: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub project_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub target_scope: serde_json::Value,
    pub tool_profile_id: Option<String>,
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFlowRequest {
    pub task_id: String,
    pub target_scope: serde_json::Value,
    pub tool_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub task_id: String,
    pub flow_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRunRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionRequest {
    pub decision: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentSessionRequest {
    pub project_id: Option<String>,
    pub objective: String,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub constraints: serde_json::Value,
    #[serde(default)]
    pub budget: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationMessageRequest {
    pub content: String,
    #[serde(default = "default_message_type")]
    pub message_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

fn default_message_type() -> String {
    "steer".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmScopeRequest {
    pub targets: Vec<String>,
    #[serde(default)]
    pub constraints: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRunRequest {
    pub name: Option<String>,
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRunInstructionRequest {
    pub instruction_type: String,
    pub content: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}
