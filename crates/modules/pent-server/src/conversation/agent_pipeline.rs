//! Provider-independent Agent middleware pipeline inspired by DesRed/Eino.

use serde_json::Value;

use super::ModelToolCall;

pub const DEFAULT_MAX_TOOL_ARGUMENT_BYTES: usize = 256 * 1024;
pub const DEFAULT_MAX_SHELL_COMMAND_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestrationMode {
    Deep,
    PlanExecute,
    Supervisor,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallValidation {
    Valid,
    Repairable(String),
    Rejected(String),
}

/// Validate freshly generated tool calls before any approval or execution.
pub fn validate_tool_call(call: &mut ModelToolCall) -> ToolCallValidation {
    if call.arguments.len() > DEFAULT_MAX_TOOL_ARGUMENT_BYTES {
        return ToolCallValidation::Rejected(format!(
            "工具参数超过 {} 字节限制",
            DEFAULT_MAX_TOOL_ARGUMENT_BYTES
        ));
    }
    let parsed = match serde_json::from_str::<Value>(&call.arguments) {
        Ok(Value::Object(object)) => Value::Object(object),
        Ok(_) => {
            call.arguments = "{}".into();
            return ToolCallValidation::Repairable("工具参数不是 JSON 对象，已替换为空对象".into());
        }
        Err(_) => {
            call.arguments = "{}".into();
            return ToolCallValidation::Repairable("工具参数 JSON 损坏，已替换为空对象".into());
        }
    };
    if matches!(
        call.name.trim().to_ascii_lowercase().as_str(),
        "exec" | "execute"
    ) {
        let command = parsed.get("command").and_then(Value::as_str).unwrap_or("");
        if command.len() > DEFAULT_MAX_SHELL_COMMAND_BYTES {
            return ToolCallValidation::Rejected(format!(
                "Shell 命令超过 {} 字节限制；应先写入文件再执行短命令",
                DEFAULT_MAX_SHELL_COMMAND_BYTES
            ));
        }
    }
    ToolCallValidation::Valid
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

/// Preserve the first and latest user inputs when enriching sub-agent tasks.
pub fn user_context_supplement(
    messages: &[kairos_platform_pent_api::AgentMessage],
    max_chars: usize,
) -> String {
    let user: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content.trim())
        .filter(|m| !m.is_empty())
        .collect();
    if user.is_empty() || max_chars == 0 {
        return String::new();
    }
    let combined = if user.len() == 1 {
        user[0].to_string()
    } else {
        format!(
            "[最初目标]\n{}\n\n[当前指令]\n{}",
            user[0],
            user[user.len() - 1]
        )
    };
    let clipped: String = combined.chars().take(max_chars).collect();
    format!("\n\n## 用户历史输入（原文，子代理必读）\n{clipped}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repairs_non_object_arguments() {
        let mut call = ModelToolCall {
            id: "1".into(),
            name: "nmap".into(),
            arguments: "[]".into(),
        };
        assert!(matches!(
            validate_tool_call(&mut call),
            ToolCallValidation::Repairable(_)
        ));
        assert_eq!(call.arguments, "{}");
    }
    #[test]
    fn classifies_retryable_errors() {
        assert!(is_transient_model_error(&anyhow::anyhow!(
            "HTTP 503 service unavailable"
        )));
        assert!(!is_transient_model_error(&anyhow::anyhow!(
            "HTTP 401 invalid api key"
        )));
    }
}
