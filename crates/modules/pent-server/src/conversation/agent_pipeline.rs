//! Provider-independent Agent middleware pipeline inspired by DesRed/Eino.

use kairos_platform_pent_api::{RuntimeAgentStatus, RuntimeSubAgent};
use kairos_platform_pent_runtime::ports::PlanProgress;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestrationMode {
    Deep,
    PlanExecute,
    Supervisor,
}

pub fn multi_agent_enabled(policy: &Value) -> bool {
    policy
        .pointer("/multi_agent/multi_agent_enabled")
        .or_else(|| policy.get("multi_agent_enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

impl OrchestrationMode {
    pub fn from_policy(policy: &Value) -> Self {
        match policy
            .pointer("/multi_agent/orchestration")
            .or_else(|| policy.get("orchestration"))
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("plan_execute") | Some("plan-execute") => Self::PlanExecute,
            Some("supervisor") => Self::Supervisor,
            _ => Self::Deep,
        }
    }

    pub fn instruction(self) -> &'static str {
        match self {
            Self::Deep => {
                "编排模式为 deep：主代理负责分解目标，可按需并行委派专业子代理，并汇总可验证证据。"
            }
            Self::PlanExecute => {
                "编排模式为 plan_execute：先形成可检查的步骤计划；每次只执行当前步骤，依据结果更新或重排剩余计划，完成后统一答复。"
            }
            Self::Supervisor => {
                "编排模式为 supervisor：主代理只负责路由、监督和汇总；专业任务优先通过 agent_spawn 委派，并用 agent_collect 收集结果。"
            }
        }
    }
}

pub fn is_transient_model_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    if [
        "401",
        "403",
        "invalid api key",
        "context canceled",
        "deadline exceeded",
    ]
    .iter()
    .any(|marker| message.contains(marker))
    {
        return false;
    }
    [
        "429",
        "too many requests",
        "rate limit",
        "overloaded",
        "capacity",
        "temporarily unavailable",
        "service unavailable",
        "bad gateway",
        "gateway timeout",
        "internal server error",
        "connection reset",
        "connection refused",
        "connection closed",
        "closed connection",
        "peer closed",
        "close_notify",
        "i/o timeout",
        "network is unreachable",
        "broken pipe",
        "tls handshake timeout",
        "unexpected eof",
        "unexpectedeof",
        "stream error",
        "http 500",
        "http 502",
        "http 503",
        "http 504",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

/// Errors that cannot recover through backoff and must be surfaced as model
/// configuration failures. Keeping this separate from the transient matcher
/// avoids retrying authentication failures while still allowing an unknown
/// transport error to use the bounded Run retry policy.
pub fn is_permanent_model_configuration_error(error: &str) -> bool {
    let message = error.to_ascii_lowercase();
    [
        "http 401",
        "status: 401",
        "401 unauthorized",
        "http 403",
        "status: 403",
        "403 forbidden",
        "invalid authentication",
        "authentication fails",
        "authentication failed",
        "invalid api key",
        "incorrect api key",
        "api key not valid",
        "api key is required",
        "missing api key",
        "insufficient_quota",
        "billing quota",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

pub fn plan_completion_rejection(progress: &PlanProgress) -> Option<String> {
    (!progress.is_complete()).then(|| {
        format!(
            "计划尚未收敛：总计 {}，待执行 {}，进行中 {}，阻塞 {}；请继续执行或显式更新步骤状态",
            progress.total, progress.pending, progress.active, progress.blocked
        )
    })
}

pub fn supervisor_completion_rejection(agents: &[RuntimeSubAgent]) -> Option<String> {
    if agents.is_empty() {
        return Some("supervisor 尚未委派任何专业子代理".into());
    }
    let running = agents
        .iter()
        .filter(|agent| agent.status == RuntimeAgentStatus::Running)
        .count();
    if running > 0 {
        return Some(format!("仍有 {running} 个子代理运行中，请先 agent_collect"));
    }
    if !agents
        .iter()
        .any(|agent| agent.status == RuntimeAgentStatus::Completed)
    {
        return Some("所有子代理均未成功完成，请重新委派或说明 NO_PATH".into());
    }
    let uncollected = agents.iter().filter(|agent| !agent.collected).count();
    if uncollected > 0 {
        return Some(format!(
            "仍有 {uncollected} 个子代理结果未收集，请逐一调用 agent_collect 后再汇总"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifies_retryable_errors() {
        assert!(is_transient_model_error(&anyhow::anyhow!(
            "HTTP 503 service unavailable"
        )));
        assert!(is_transient_model_error(&anyhow::anyhow!(
            "读取模型响应失败: reqwest UnexpectedEof: peer closed connection without sending TLS close_notify"
        )));
        assert!(!is_transient_model_error(&anyhow::anyhow!(
            "HTTP 401 invalid api key"
        )));
        assert!(is_permanent_model_configuration_error(
            "模型调用失败: Invalid Authentication"
        ));
        assert!(is_permanent_model_configuration_error(
            "HTTP 401 Unauthorized: Authentication Fails (governor)"
        ));
        assert!(!is_permanent_model_configuration_error(
            "HTTP 503 service unavailable"
        ));
    }

    #[test]
    fn honors_multi_agent_enabled_policy() {
        assert!(!multi_agent_enabled(&serde_json::json!({
            "multi_agent": {"multi_agent_enabled": false}
        })));
        assert!(multi_agent_enabled(&serde_json::json!({
            "multi_agent": {"multi_agent_enabled": true}
        })));
        assert!(multi_agent_enabled(&serde_json::json!({})));
    }

    #[test]
    fn plan_completion_requires_terminal_steps() {
        assert!(
            plan_completion_rejection(&PlanProgress {
                total: 2,
                completed: 1,
                active: 1,
                ..Default::default()
            })
            .is_some()
        );
        assert!(
            plan_completion_rejection(&PlanProgress {
                total: 2,
                completed: 1,
                failed: 1,
                ..Default::default()
            })
            .is_none()
        );
    }

    #[test]
    fn supervisor_completion_requires_successful_settled_delegate() {
        let agent = |status| RuntimeSubAgent {
            collected: status == RuntimeAgentStatus::Completed,
            agent_id: "agent-1".into(),
            description: "test".into(),
            status,
            result: None,
            error: None,
            started_at: "now".into(),
            completed_at: None,
        };
        assert!(supervisor_completion_rejection(&[]).is_some());
        assert!(supervisor_completion_rejection(&[agent(RuntimeAgentStatus::Running)]).is_some());
        assert!(supervisor_completion_rejection(&[agent(RuntimeAgentStatus::Failed)]).is_some());
        assert!(supervisor_completion_rejection(&[agent(RuntimeAgentStatus::Completed)]).is_none());
        let mut uncollected = agent(RuntimeAgentStatus::Completed);
        uncollected.collected = false;
        assert!(supervisor_completion_rejection(&[uncollected]).is_some());
    }
}
