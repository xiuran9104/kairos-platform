//! Application use cases — create Run, execute, cancel, approve.

use chrono::Utc;
use kairos_platform_pent_api::commands::*;
use kairos_platform_pent_api::models::{
    ExecutionPolicy, FlowStatus, PentFlow, PentRun, PentSubtask, PentTask, RunStatus,
    SubtaskStatus, TaskPriority, TaskStatus,
};
use uuid::Uuid;

/// Create a new task.
pub fn create_task(req: CreateTaskRequest, tenant_id: i64, user_id: &str) -> PentTask {
    let now = Utc::now();
    let priority = match req.priority.as_deref() {
        Some("critical") => TaskPriority::Critical,
        Some("high") => TaskPriority::High,
        Some("low") => TaskPriority::Low,
        _ => TaskPriority::Medium,
    };
    PentTask {
        id: Uuid::new_v4().to_string(),
        tenant_id,
        project_id: req.project_id,
        name: req.name,
        description: req.description,
        target_scope: req.target_scope,
        tool_profile_id: req.tool_profile_id,
        priority,
        status: TaskStatus::Draft,
        created_by: user_id.to_string(),
        created_at: now,
        updated_at: now,
    }
}

/// Create a new run from a task.
pub fn create_run(task: &PentTask, flow_id: Option<String>, user_id: &str) -> PentRun {
    let now = Utc::now();
    PentRun {
        id: Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        session_id: None,
        flow_id,
        tenant_id: task.tenant_id,
        user_id: user_id.to_string(),
        project_id: task.project_id.clone(),
        objective_snapshot: task.description.clone(),
        scope_snapshot: task.target_scope.clone(),
        policy_snapshot: serde_json::json!({}),
        budget_snapshot: serde_json::json!({}),
        status: RunStatus::Queued,
        worker_id: None,
        lease_expires_at: None,
        started_at: None,
        completed_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Create default execution policy for a run.
pub fn default_execution_policy(task: &PentTask, user_id: &str) -> ExecutionPolicy {
    ExecutionPolicy {
        allowed_targets: task
            .target_scope
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        allowed_tools: vec!["tcp_scan".into(), "http_probe".into(), "dns_lookup".into()],
        ask_tools: vec!["ssh_exec".into(), "sql_query".into()],
        denied_tools: vec!["shell_exec".into()],
        max_concurrency: 3,
        timeout_seconds: 3600,
        max_retries: 2,
        initiator_id: user_id.to_string(),
        data_scope: "project".into(),
        approval_rules: None,
        policy_version: 1,
    }
}

/// Generate an initial flow plan for a task.
pub fn plan_flow(task: &PentTask, _policy: &ExecutionPolicy) -> PentFlow {
    let now = Utc::now();
    let plan = serde_json::json!({
        "phases": [
            {"name": "recon", "tools": ["tcp_scan", "dns_lookup"]},
            {"name": "probe", "tools": ["http_probe"]},
            {"name": "analyze", "tools": []}
        ]
    });
    PentFlow {
        id: Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        plan,
        status: FlowStatus::Planned,
        version: 1,
        policy_snapshot: default_execution_policy(task, &task.created_by),
        created_at: now,
        updated_at: now,
    }
}

/// Build subtasks from a flow plan.
pub fn build_subtasks(flow: &PentFlow, run_id: &str) -> Vec<PentSubtask> {
    let now = Utc::now();
    let mut subtasks = Vec::new();
    if let Some(phases) = flow.plan.get("phases").and_then(|p| p.as_array()) {
        for (i, phase) in phases.iter().enumerate() {
            if let Some(tools) = phase.get("tools").and_then(|t| t.as_array()) {
                for (j, tool) in tools.iter().enumerate() {
                    if let Some(tool_name) = tool.as_str() {
                        subtasks.push(PentSubtask {
                            id: Uuid::new_v4().to_string(),
                            flow_id: flow.id.clone(),
                            run_id: run_id.to_string(),
                            name: format!(
                                "{}-{}",
                                phase.get("name").and_then(|n| n.as_str()).unwrap_or("step"),
                                tool_name
                            ),
                            tool_name: tool_name.to_string(),
                            params: serde_json::json!({}),
                            status: SubtaskStatus::Pending,
                            sequence: (i * 100 + j) as i32,
                            retry_count: 0,
                            created_at: now,
                            updated_at: now,
                        });
                    }
                }
            }
        }
    }
    subtasks
}
