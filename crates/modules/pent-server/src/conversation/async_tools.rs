//! Durable, model-visible control plane for long-running security tools.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use kairos_platform_pent_runtime::ports::{
    ConversationRepository, ConversationToolExecutor, RuntimeToolCall, ToolExecutionContext,
};
use serde_json::{Value, json};

const DEFAULT_WAIT_SECONDS: u64 = 20;
const MAX_WAIT_SECONDS: u64 = 60;

#[derive(Clone, Default)]
pub(super) struct AsyncToolRegistry {
    tasks: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
}

impl AsyncToolRegistry {
    /// Share cancellation handles across conversation-runtime instances. A
    /// runtime is rebuilt for every API request, while detached executions may
    /// outlive that request and still need to be cancellable on the next turn.
    pub fn global() -> Self {
        static REGISTRY: OnceLock<AsyncToolRegistry> = OnceLock::new();
        REGISTRY.get_or_init(Self::default).clone()
    }

    pub async fn start(
        &self,
        repository: Arc<dyn ConversationRepository>,
        executor: Arc<dyn ConversationToolExecutor>,
        context: ToolExecutionContext,
        tool_name: String,
        arguments: String,
    ) -> anyhow::Result<Value> {
        anyhow::ensure!(context.run_id.is_some(), "后台工具只能在已有 Run 中执行");
        anyhow::ensure!(
            !is_control_tool(&tool_name),
            "后台执行不能嵌套调用 execution 控制工具"
        );
        serde_json::from_str::<Value>(&arguments)
            .map_err(|error| anyhow::anyhow!("arguments 必须是 JSON 对象: {error}"))?;
        let run_id = context.run_id.clone().unwrap_or_default();
        let execution_id = uuid::Uuid::new_v4().to_string();
        repository
            .begin_background_tool_execution(&run_id, &execution_id, &tool_name, &arguments)
            .await?;

        let task_execution_id = execution_id.clone();
        let task_tool_name = tool_name.clone();
        let task_arguments = arguments.clone();
        let task_run_id = run_id.clone();
        let tasks = self.tasks.clone();
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            // Install the abort handle before execution starts, including for
            // tools that can complete immediately.
            if start_rx.await.is_err() {
                return;
            }
            let started = Instant::now();
            let result = executor
                .execute(
                    &RuntimeToolCall {
                        id: task_execution_id.clone(),
                        name: task_tool_name.clone(),
                        arguments: task_arguments.clone(),
                    },
                    &context,
                )
                .await;
            let evidence = repository
                .record_tool_result(
                    &task_run_id,
                    &task_execution_id,
                    &task_tool_name,
                    &task_arguments,
                    result.success,
                    &result.content,
                    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                )
                .await;
            if let Err(error) = &evidence {
                tracing::error!(
                    execution_id = %task_execution_id,
                    tool = %task_tool_name,
                    %error,
                    "failed to persist detached tool result"
                );
            }
            if result.success
                && matches!(
                    task_tool_name.as_str(),
                    "http_request" | "http_probe" | "http-framework-test"
                )
            {
                repository
                    .record_http_traffic(
                        &task_run_id,
                        &task_execution_id,
                        &task_tool_name,
                        &task_arguments,
                        &result.content,
                        None,
                    )
                    .await
                    .ok();
            }
            if let Ok(mut guard) = tasks.lock() {
                guard.remove(&task_execution_id);
            }
        });
        if let Ok(mut guard) = self.tasks.lock() {
            guard.insert(execution_id.clone(), task.abort_handle());
        }
        let _ = start_tx.send(());
        Ok(json!({
            "execution_id": execution_id,
            "tool_name": tool_name,
            "status": "running",
            "instruction": "工具已在后台执行。继续其他独立测试；需要结果时调用 get_tool_execution 或 wait_tool_execution。最终报告前必须收集或取消所有后台 execution。"
        }))
    }

    pub async fn get(
        &self,
        repository: &dyn ConversationRepository,
        run_id: &str,
        arguments: &str,
    ) -> anyhow::Result<Value> {
        let parsed = parse_control_arguments(arguments)?;
        repository
            .execute_evidence_tool(
                run_id,
                "query_execution_result",
                &json!({
                    "execution_id": execution_id(&parsed)?,
                    "page": parsed.get("page").and_then(Value::as_u64).unwrap_or(1),
                    "limit": parsed.get("limit").and_then(Value::as_u64).unwrap_or(100),
                    "search": parsed.get("search").cloned().unwrap_or(Value::Null)
                })
                .to_string(),
            )
            .await
    }

    pub async fn wait(
        &self,
        repository: &dyn ConversationRepository,
        run_id: &str,
        arguments: &str,
    ) -> anyhow::Result<Value> {
        let parsed = parse_control_arguments(arguments)?;
        let wait_seconds = parsed
            .get("timeout_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_WAIT_SECONDS)
            .clamp(1, MAX_WAIT_SECONDS);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(wait_seconds);
        loop {
            let snapshot = self.get(repository, run_id, arguments).await?;
            if snapshot.get("status").and_then(Value::as_str) != Some("running") {
                return Ok(snapshot);
            }
            if tokio::time::Instant::now() >= deadline {
                let mut bounded = snapshot;
                bounded["wait_timed_out"] = Value::Bool(true);
                bounded["instruction"] = Value::String(
                    "本次有界等待结束，execution 仍在运行；可继续其他测试，稍后再次 wait，或取消。"
                        .into(),
                );
                return Ok(bounded);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn cancel(
        &self,
        repository: &dyn ConversationRepository,
        run_id: &str,
        arguments: &str,
    ) -> anyhow::Result<Value> {
        let parsed = parse_control_arguments(arguments)?;
        let execution_id = execution_id(&parsed)?;
        let reason = parsed
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("cancelled by agent")
            .trim();
        let cancelled = repository
            .cancel_background_tool_execution(run_id, execution_id, reason)
            .await?;
        // Authorize cancellation through the run-scoped durable update before
        // touching the process-global handle. This prevents one run from
        // cancelling another run's execution by guessing its id.
        if cancelled
            && let Some(handle) = self
                .tasks
                .lock()
                .ok()
                .and_then(|mut guard| guard.remove(execution_id))
        {
            handle.abort();
        }
        Ok(json!({
            "execution_id": execution_id,
            "status": if cancelled { "cancelled" } else { "already_finished" },
            "cancelled": cancelled
        }))
    }
}

fn parse_control_arguments(arguments: &str) -> anyhow::Result<Value> {
    serde_json::from_str(arguments)
        .map_err(|error| anyhow::anyhow!("execution 控制参数不是有效 JSON: {error}"))
}

fn execution_id(arguments: &Value) -> anyhow::Result<&str> {
    arguments
        .get("execution_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 100)
        .ok_or_else(|| anyhow::anyhow!("缺少有效 execution_id"))
}

pub(super) fn is_control_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "start_tool_execution"
            | "get_tool_execution"
            | "wait_tool_execution"
            | "cancel_tool_execution"
    )
}

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "type":"function",
            "function":{
                "name":"start_tool_execution",
                "description":"在后台启动一个预计耗时较长的已授权安全工具，立即返回 execution_id。适用于 nuclei、sqlmap、目录爆破和大型枚举；启动后继续执行其他独立验证，最终报告前必须 wait/get 收集结果。底层工具仍执行 Scope、审批和沙箱策略。",
                "parameters":{
                    "type":"object","additionalProperties":false,
                    "required":["tool_name","arguments"],
                    "properties":{
                        "tool_name":{"type":"string","description":"已经通过 tool_search 看过完整 schema 的精确工具名"},
                        "arguments":{"type":"object","description":"完全符合目标工具 schema 的参数对象"}
                    }
                }
            }
        }),
        json!({
            "type":"function",
            "function":{
                "name":"get_tool_execution",
                "description":"立即查询当前 Run 中后台 execution 的状态和已完成结果，不阻塞。",
                "parameters":execution_query_schema(false)
            }
        }),
        json!({
            "type":"function",
            "function":{
                "name":"wait_tool_execution",
                "description":"有界等待当前 Run 中后台 execution 完成；到时仍在运行会返回 running，之后可再次等待。",
                "parameters":execution_query_schema(true)
            }
        }),
        json!({
            "type":"function",
            "function":{
                "name":"cancel_tool_execution",
                "description":"取消当前 Run 中仍在运行的后台 execution；用于误调用、长期无进展或策略调整。",
                "parameters":{
                    "type":"object","additionalProperties":false,"required":["execution_id"],
                    "properties":{
                        "execution_id":{"type":"string"},
                        "reason":{"type":"string","description":"取消原因"}
                    }
                }
            }
        }),
    ]
}

fn execution_query_schema(include_wait: bool) -> Value {
    let mut properties = serde_json::Map::from_iter([
        ("execution_id".into(), json!({"type":"string"})),
        ("page".into(), json!({"type":"integer","minimum":1})),
        (
            "limit".into(),
            json!({"type":"integer","minimum":1,"maximum":500}),
        ),
        ("search".into(), json!({"type":"string"})),
    ]);
    if include_wait {
        properties.insert(
            "timeout_seconds".into(),
            json!({"type":"integer","minimum":1,"maximum":60,"default":20}),
        );
    }
    json!({
        "type":"object","additionalProperties":false,"required":["execution_id"],
        "properties":properties
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_bounded_execution_control_schemas() {
        let definitions = definitions();
        assert_eq!(definitions.len(), 4);
        assert_eq!(
            definitions[2]["function"]["parameters"]["properties"]["timeout_seconds"]["maximum"],
            60
        );
        assert!(is_control_tool("cancel_tool_execution"));
        assert!(!is_control_tool("nuclei"));
    }
}
