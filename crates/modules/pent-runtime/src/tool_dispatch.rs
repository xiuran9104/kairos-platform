//! Shared tool-call preparation and dispatch classification.
//!
//! Execution remains adapter-owned. This module validates model arguments,
//! applies orchestration constraints and crosses the approval gate before any
//! main or sub-agent tool is executed.

use crate::ports::{ModelToolCall, ToolApprovalGate, ToolApprovalOutcome, ToolApprovalRequest};

pub use crate::ports::ToolExecutionClass;

pub const DEFAULT_MAX_TOOL_ARGUMENT_BYTES: usize = 256 * 1024;
pub const DEFAULT_MAX_SHELL_COMMAND_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default)]
pub struct ToolDispatchPolicy {
    pub agent_management_only: bool,
    pub plan_required: bool,
    /// A persisted plan exists but currently has no ready/running step.
    pub plan_step_blocked: bool,
}

#[derive(Debug, Clone)]
pub struct ToolDispatchContext {
    pub run_id: Option<String>,
    pub session_id: String,
    pub policy: ToolDispatchPolicy,
}

#[derive(Debug, Clone)]
pub struct ToolDispatchDecision {
    pub tool_call_id: String,
    pub class: ToolExecutionClass,
    pub repair: Option<String>,
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolDispatchPlan {
    pub decisions: Vec<ToolDispatchDecision>,
}

impl ToolDispatchPlan {
    pub fn decision(&self, tool_call_id: &str) -> Option<&ToolDispatchDecision> {
        self.decisions
            .iter()
            .find(|decision| decision.tool_call_id == tool_call_id)
    }

    pub fn blocked_reason(&self, tool_call_id: &str) -> Option<&str> {
        self.decision(tool_call_id)
            .and_then(|decision| decision.blocked.as_deref())
    }
}

pub struct ToolDispatcher<'a> {
    approval_gate: &'a dyn ToolApprovalGate,
}

impl<'a> ToolDispatcher<'a> {
    pub fn new(approval_gate: &'a dyn ToolApprovalGate) -> Self {
        Self { approval_gate }
    }

    pub async fn prepare(
        &self,
        calls: &mut [ModelToolCall],
        context: &ToolDispatchContext,
    ) -> ToolDispatchPlan {
        let mut plan = ToolDispatchPlan {
            decisions: calls
                .iter_mut()
                .map(|call| prepare_call(call, &context.policy))
                .collect(),
        };

        let Some(run_id) = context.run_id.as_deref() else {
            return plan;
        };
        for (call, decision) in calls.iter_mut().zip(plan.decisions.iter_mut()) {
            if decision.blocked.is_some() {
                continue;
            }
            match self
                .approval_gate
                .review(&ToolApprovalRequest {
                    run_id: run_id.to_string(),
                    session_id: context.session_id.clone(),
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .await
            {
                Ok(ToolApprovalOutcome::Approved(arguments)) => {
                    call.arguments = arguments;
                }
                Ok(ToolApprovalOutcome::Rejected(reason)) => {
                    decision.blocked = Some(reason);
                }
                Err(error) => {
                    decision.blocked = Some(format!("AI 围栏处理失败：{error}"));
                }
            }
        }
        plan
    }
}

fn prepare_call(call: &mut ModelToolCall, policy: &ToolDispatchPolicy) -> ToolDispatchDecision {
    let mut decision = ToolDispatchDecision {
        tool_call_id: call.id.clone(),
        class: call.execution_class,
        repair: None,
        blocked: policy_rejection(&call.name, policy),
    };
    if decision.blocked.is_some() {
        return decision;
    }
    match validate_tool_call(call) {
        ToolCallValidation::Valid => {}
        ToolCallValidation::Repairable(reason) => decision.repair = Some(reason),
        ToolCallValidation::Rejected(reason) => decision.blocked = Some(reason),
    }
    decision
}

fn policy_rejection(tool_name: &str, policy: &ToolDispatchPolicy) -> Option<String> {
    if policy.agent_management_only && !is_agent_management_tool(tool_name) {
        return Some(
            "supervisor 模式的主代理只能路由、监督和汇总子代理；请使用 agent_spawn/agent_collect"
                .into(),
        );
    }
    if policy.plan_required && tool_name != "plan_create" {
        return Some("plan_execute 模式必须先调用 plan_create 创建持久化计划".into());
    }
    if policy.plan_step_blocked && !matches!(tool_name, "plan_get" | "plan_update") {
        return Some(
            "plan_execute 模式当前没有 ready/running 步骤；请先用 plan_get 检查并通过 plan_update 重排计划"
                .into(),
        );
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ToolCallValidation {
    Valid,
    Repairable(String),
    Rejected(String),
}

fn validate_tool_call(call: &mut ModelToolCall) -> ToolCallValidation {
    if call.arguments.len() > DEFAULT_MAX_TOOL_ARGUMENT_BYTES {
        return ToolCallValidation::Rejected(format!(
            "工具参数超过 {} 字节限制",
            DEFAULT_MAX_TOOL_ARGUMENT_BYTES
        ));
    }
    let parsed = match serde_json::from_str::<serde_json::Value>(&call.arguments) {
        Ok(serde_json::Value::Object(object)) => serde_json::Value::Object(object),
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
        let command = parsed
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if command.len() > DEFAULT_MAX_SHELL_COMMAND_BYTES {
            return ToolCallValidation::Rejected(format!(
                "Shell 命令超过 {} 字节限制；应先写入文件再执行短命令",
                DEFAULT_MAX_SHELL_COMMAND_BYTES
            ));
        }
    }
    ToolCallValidation::Valid
}

fn is_agent_management_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "agent_spawn" | "agent_status" | "agent_collect" | "agent_list"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: &str, execution_class: ToolExecutionClass) -> ModelToolCall {
        ModelToolCall {
            id: format!("call-{name}"),
            name: name.into(),
            arguments: arguments.into(),
            execution_class,
        }
    }

    #[test]
    fn repairs_arguments_and_classifies_calls() {
        let mut inline = call("custom_reference", "[]", ToolExecutionClass::Inline);
        let inline_decision = prepare_call(&mut inline, &ToolDispatchPolicy::default());
        let mut destructive = call("custom_scanner", "{}", ToolExecutionClass::Destructive);
        let destructive_decision = prepare_call(&mut destructive, &ToolDispatchPolicy::default());

        assert_eq!(inline.arguments, "{}");
        assert!(inline_decision.repair.is_some());
        assert_eq!(inline_decision.class, ToolExecutionClass::Inline);
        assert_eq!(destructive_decision.class, ToolExecutionClass::Destructive);
    }

    #[test]
    fn orchestration_policy_blocks_before_approval() {
        let policy = ToolDispatchPolicy {
            agent_management_only: true,
            plan_required: false,
            plan_step_blocked: false,
        };
        let mut scan = call("nmap", "{}", ToolExecutionClass::ReadOnly);
        let mut spawn = call("agent_spawn", "{}", ToolExecutionClass::Inline);

        assert!(prepare_call(&mut scan, &policy).blocked.is_some());
        assert!(prepare_call(&mut spawn, &policy).blocked.is_none());
    }

    #[test]
    fn plan_execute_blocks_tools_without_an_executable_step() {
        let policy = ToolDispatchPolicy {
            plan_step_blocked: true,
            ..Default::default()
        };
        let mut scan = call("nmap", "{}", ToolExecutionClass::ReadOnly);
        let mut inspect = call("plan_get", "{}", ToolExecutionClass::Inline);
        let mut replan = call("plan_update", "{}", ToolExecutionClass::Inline);

        assert!(prepare_call(&mut scan, &policy).blocked.is_some());
        assert!(prepare_call(&mut inspect, &policy).blocked.is_none());
        assert!(prepare_call(&mut replan, &policy).blocked.is_none());
    }

    #[test]
    fn dispatch_uses_catalog_execution_metadata() {
        let mut call = call("same_name", "{}", ToolExecutionClass::Destructive);
        let decision = prepare_call(&mut call, &ToolDispatchPolicy::default());

        assert_eq!(decision.class, ToolExecutionClass::Destructive);
    }

    #[test]
    fn oversized_shell_command_is_blocked_before_approval() {
        let arguments = serde_json::json!({
            "command": "x".repeat(DEFAULT_MAX_SHELL_COMMAND_BYTES + 1)
        })
        .to_string();
        let mut shell = call("exec", &arguments, ToolExecutionClass::Destructive);
        let decision = prepare_call(&mut shell, &ToolDispatchPolicy::default());

        assert!(decision.blocked.is_some());
    }
}
