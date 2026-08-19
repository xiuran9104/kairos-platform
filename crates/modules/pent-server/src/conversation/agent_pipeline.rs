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

/// Minimum prompt length for a delegation handoff. Shorter prompts almost
/// never carry the action, verification method and success criteria a
/// sub-agent needs to act without guessing.
const MIN_DELEGATION_PROMPT_CHARS: usize = 40;

/// Validate that an `agent_spawn` delegation carries an actionable handoff
/// package: a concrete target (inline URL/IP/domain, or the explicit `target`
/// field) plus a substantive task description. Sub-agents run without parent
/// context, so a vague delegation makes them guess targets instead of
/// executing — this mirrors DesRed's "return the missing-info list instead of
/// guessing" rule, enforced at the spawn boundary.
pub fn delegation_handoff_rejection(
    description: &str,
    prompt: &str,
    target: Option<&str>,
) -> Option<String> {
    let mut missing = Vec::new();
    if prompt.trim().chars().count() < MIN_DELEGATION_PROMPT_CHARS {
        missing.push("任务描述过短（需说明要执行的验证动作与成功标准）");
    }
    let has_inline_target =
        contains_concrete_target(description) || contains_concrete_target(prompt);
    if target.map(str::trim).unwrap_or_default().is_empty() && !has_inline_target {
        missing.push("缺少具体目标（URL、IP 或域名；也可用 target 字段单独传入）");
    }
    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "委派交接包不完整：{}。子代理不继承父上下文，禁止猜测目标；请补齐后重试。\
             示例：target=\"http://host/login\"，prompt 写明要验证的漏洞类型、使用的工具与成功标准（怎样的证据算确认）",
            missing.join("；")
        ))
    }
}

fn contains_concrete_target(text: &str) -> bool {
    if text.contains("http://") || text.contains("https://") {
        return true;
    }
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':'))
        .any(|token| looks_like_host(token.trim_matches(['.', ':'])))
}

fn looks_like_host(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let host = token.split(':').next().unwrap_or(token);
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() == 4
        && labels
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return true;
    }
    labels.len() >= 2
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        && labels
            .last()
            .is_some_and(|tld| tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic()))
}

pub fn supervisor_completion_rejection(agents: &[RuntimeSubAgent]) -> Option<String> {
    if agents.is_empty() {
        return Some(
            "supervisor 尚未委派任何专业子代理；请先 agent_spawn 委派专项子代理执行".into(),
        );
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
    fn delegation_handoff_requires_substance_and_concrete_target() {
        // 过短 + 无目标:两个缺失一起报
        let rejection =
            delegation_handoff_rejection("扫描一下", "继续测试", None).expect("must reject");
        assert!(rejection.contains("任务描述过短"));
        assert!(rejection.contains("缺少具体目标"));

        // 引用了"主目标"但无具体目标:拒绝(禁止猜测)
        assert!(
            delegation_handoff_rejection(
                "验证 SQL 注入",
                "对主目标登录接口验证 SQL 注入,记录证据",
                None,
            )
            .is_some()
        );

        // target 字段单独传入即可满足目标要求
        assert!(
            delegation_handoff_rejection(
                "验证登录接口 SQL 注入",
                "使用 sqlmap_scan 对登录接口 username 参数做只读验证,稳定差异即确认",
                Some("http://target.local/login"),
            )
            .is_none()
        );

        // prompt 内联 URL / IP 也算具体目标
        assert!(
            delegation_handoff_rejection(
                "枚举目录",
                "对 https://shop.example.com/admin 做低速定向字典枚举,先建立 404 基线",
                None,
            )
            .is_none()
        );
        assert!(
            delegation_handoff_rejection(
                "端口复核",
                "对 10.0.0.8:8080 的 HTTP 服务做指纹复核，验证常见管理端点并记录响应证据",
                None,
            )
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
