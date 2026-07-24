//! Deterministic feedback from normalized tool results to the planner.

use kairos_platform_pent_api::{ToolExecutionResult, ToolExecutionStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanningDirective {
    Continue,
    Replan,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanningFeedback {
    pub directive: PlanningDirective,
    pub reason: String,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedToolCall {
    pub tool_name: String,
    pub target: String,
    pub reason: String,
}

pub fn derive_planning_feedback(result: &ToolExecutionResult) -> PlanningFeedback {
    match result.status {
        ToolExecutionStatus::Failed | ToolExecutionStatus::Blocked => PlanningFeedback {
            directive: PlanningDirective::Replan,
            reason: format!("工具 {} 未成功执行", result.tool_name),
            suggested_actions: vec!["检查工具依赖和参数".into(), "选择等价低风险工具".into()],
        },
        _ if result
            .findings
            .iter()
            .any(|finding| matches!(finding.severity.as_str(), "critical" | "high")) =>
        {
            PlanningFeedback {
                directive: PlanningDirective::Continue,
                reason: "发现高风险线索，需要在授权范围内验证并固化证据".into(),
                suggested_actions: vec!["请求验证步骤".into(), "保存证据".into()],
            }
        }
        _ if !result.observations.is_empty() => PlanningFeedback {
            directive: PlanningDirective::Continue,
            reason: "获得新的结构化观察".into(),
            suggested_actions: vec!["根据观察扩展下一步探测".into()],
        },
        _ => PlanningFeedback {
            directive: PlanningDirective::Stop,
            reason: "没有获得可继续推进的观察或发现".into(),
            suggested_actions: vec![],
        },
    }
}

/// Selects one bounded next step from structured observations.
pub fn plan_next_tool(
    result: &ToolExecutionResult,
    completed_tools: &[String],
) -> Option<PlannedToolCall> {
    if result.status == ToolExecutionStatus::Failed
        || result.status == ToolExecutionStatus::Blocked
        || completed_tools.len() >= 3
    {
        return None;
    }
    let already_completed = |tool: &str| completed_tools.iter().any(|value| value == tool);
    if matches!(result.tool_name.as_str(), "tcp_scan" | "nmap") && !already_completed("http_probe")
    {
        let ports = result
            .observations
            .iter()
            .find(|observation| observation.key == "ports")?
            .value
            .as_array()?;
        let has_port = |expected| {
            ports
                .iter()
                .any(|port| port.get("port").and_then(|value| value.as_u64()) == Some(expected))
        };
        let scheme = if has_port(443) {
            "https"
        } else if has_port(80) {
            "http"
        } else {
            return None;
        };
        return Some(PlannedToolCall {
            tool_name: "http_probe".into(),
            target: format!("{scheme}://{}", result.target),
            reason: "端口扫描发现 Web 服务端口，执行只读 HTTP 探测".into(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_platform_pent_api::{ToolExecutionMetrics, ToolExecutionResult};

    #[test]
    fn failed_tool_requests_replanning() {
        let result = failed_result();
        assert_eq!(
            derive_planning_feedback(&result).directive,
            PlanningDirective::Replan
        );
    }

    #[test]
    fn plans_http_probe_from_web_port_observation() {
        let mut result = failed_result();
        result.status = ToolExecutionStatus::Succeeded;
        result.tool_name = "tcp_scan".into();
        result
            .observations
            .push(kairos_platform_pent_api::ToolObservation {
                category: "network".into(),
                key: "ports".into(),
                value: serde_json::json!([{"port": 443}]),
                confidence: 0.95,
            });
        let next = plan_next_tool(&result, &["tcp_scan".into()]).unwrap();
        assert_eq!(next.tool_name, "http_probe");
        assert_eq!(next.target, "https://host");
    }

    fn failed_result() -> ToolExecutionResult {
        ToolExecutionResult {
            schema_version: 1,
            tool_name: "nmap".into(),
            target: "host".into(),
            status: ToolExecutionStatus::Failed,
            summary: "failed".into(),
            observations: vec![],
            findings: vec![],
            artifacts: vec![],
            raw_output_ref: None,
            metrics: ToolExecutionMetrics {
                exit_code: 1,
                duration_ms: 1,
            },
            warnings: vec![],
            occurred_at: chrono::Utc::now(),
        }
    }
}
