//! Provider-independent Agent middleware pipeline inspired by DesRed/Eino.

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
        "i/o timeout",
        "network is unreachable",
        "broken pipe",
        "tls handshake timeout",
        "unexpected eof",
        "stream error",
        "http 500",
        "http 502",
        "http 503",
        "http 504",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifies_retryable_errors() {
        assert!(is_transient_model_error(&anyhow::anyhow!(
            "HTTP 503 service unavailable"
        )));
        assert!(!is_transient_model_error(&anyhow::anyhow!(
            "HTTP 401 invalid api key"
        )));
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
}
