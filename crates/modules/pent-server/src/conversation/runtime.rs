use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
    ConversationMessageType, ConversationTurnStatus,
};
use sha2::Digest;

use super::rmos::{
    AttackGraph, KnowledgeCache, PentestMemory,
    injector::{build_injected_context, prepare_injection},
};
use super::tool_result_processor::{
    ToolResultPersistence, ToolResultProcessingContext, ToolResultProcessor, is_evidence_tool,
    is_finding_tool, is_traffic_tool,
};
use super::{
    AgentSession, AgentSessionFactory, AgentSpawnRequest, ConversationContextSource,
    ConversationEventSink, ConversationInstructionSink, ConversationIntentDetector,
    ConversationKnowledgeSource, ConversationModelGateway, ConversationRepository,
    ConversationToolExecutor, FindingRecorder, InlineToolExecutor, ModelToolCall, ModelTurnOutput,
    PlanStore, RoleResolver, RuntimeAgentStatus, RuntimeEnrichment, RuntimeToolCall,
    RuntimeToolResult, SkillStore, ToolApprovalGate, ToolApprovalOutcome, ToolApprovalRequest,
    ToolExecutionClass, ToolExecutionContext,
};
use kairos_platform_pent_runtime::agent_loop::{
    AgentDirective, AgentLoopLimits, parse_agent_directive,
};
use kairos_platform_pent_runtime::agent_runner::{
    AgentRoundHooks, AgentRoundOutcome, AgentRunConfig, AgentRunHooks, AgentRunner,
};
use kairos_platform_pent_runtime::agent_scheduler::inherit_child_budget;
use kairos_platform_pent_runtime::context_lifecycle::ContextManager;
use kairos_platform_pent_runtime::ports::{AgentChildBudget, KnowledgeGuide};
use kairos_platform_pent_runtime::tool_dispatch::{
    ToolDispatchContext, ToolDispatchPolicy, ToolDispatcher,
};
use kairos_platform_pent_runtime::tool_execution::ToolExecutionCoordinator;

/// Maximum tool-calling rounds before forcing a stop.
/// PentAGI CLI uses 3 hours default runtime; the conversation agent
/// limits rounds instead. 50 rounds ≈ 1-2 hours of typical execution.
const MAX_TOOL_ROUNDS: usize = 50;
/// Absolute maximum runtime for a conversation turn (2 hours).
const MAX_TURN_RUNTIME_SECS: u64 = 2 * 60 * 60;
const MAX_RUN_SUMMARY_CHARS: usize = 12_000;
const DEFAULT_MAX_CONSECUTIVE_MODEL_ERRORS: usize = 3;
/// Liveness guard, not a resource budget: repeated results, failed calls, and
/// evidence-only lookups must eventually converge even when budgets are
/// explicitly unlimited.
const MAX_CONSECUTIVE_NO_PROGRESS_RESULTS: usize = 12;

/// Autonomous loop injection message sent to the model at the start of every
/// tool loop. This reminds the model it is operating in autonomous mode and
/// must directly call tools or emit lifecycle actions.
const AUTONOMOUS_LOOP_PROMPT: &str = r#"## 自主工具循环已启动

你现在处于自主执行模式。根据目标、当前证据和可用工具直接选择下一步操作：
- 需要信息 → 直接发起 Tool Calling
- 任务完成 → 输出 `FINAL: <结论>`（事实性结论必须引用 Evidence ID）
- 所有路径均失败 → 输出 `NO_PATH: <原因>` 说明已尝试内容和阻塞点
- 无可行路径 → 输出 `NO_PATH: <原因>`
- 策略失败需调整 → 输出 `REPLAN: <新策略>` 并在下一轮直接执行

## ⚠ 工具选择规则（严格遵守）
在首次执行侦察、扫描或验证工具前，必须先调用 `skill_list` 查找与当前任务匹配的规程，
再调用 `skill_load` 加载至少一个 Skill。不得跳过 Skill 直接开始操作。
当任务从侦察进入专项验证，或漏洞类型发生变化（例如 XSS 转 SQL 注入）时，必须再次用具体关键词
调用 `skill_list`，并加载对应专项 Skill；不要让一个通用 Skill 覆盖整个 Run。
不要用 curl/http_request 手工做所有事！每个任务有专用工具：
- 端口扫描 → nmap（不要 curl 逐个端口试）
- 漏洞扫描 → nuclei / web_vuln_scan（不要 curl 手工测）
- XSS 检测 → dalfox / web_xss_scan（不要 curl 试 payload）
- SQL 注入 → sqlmap / sqlmap_scan（不要 curl 手工注入）
- 目录爆破 → ffuf / gobuster / dir_fuzz（不要手工猜路径）
- 技术栈识别 → whatweb（不要手工看响应头）
- 子域名枚举 → subfinder / amass（不要手工 DNS）
- CSP 头分析 → csp_analyze（不要手工读 header）
- Web/JS 攻击面 → hakrawler / katana；需要专项 JS 能力时先 tool_search（不要手工猜接口）
- Wiki 知识查询 → knowledge_search（不要凭记忆猜）
- 浏览器操作 → mcp_navigate_page / mcp_take_screenshot（不要 curl）

每次执行前先问自己：有没有专用工具能做这件事？当前目录里没有时调用 tool_search；有就用专用工具，不要用 curl/http_request 替代。

## 结束条件与停止约束（FINAL/NO_PATH 前逐项自检）
- 每个声称的漏洞必须已通过 record_finding 落库并引用 Evidence ID；无证据的结论一律降级为未验证线索。
- 空结果、404、超时不得直接下“不存在”结论：必须换策略（换工具/换 payload 家族/换枚举维度）再验证一次。
- 同一工具同类参数连续失败时立即停止重试，按运行时提示切换策略。
- 确认无漏洞时也要说明覆盖了哪些攻击面、用什么证据排除。

不要输出等待确认的计划、不要枚举工具列表、不要在工具调用前请求许可。"#;

#[derive(Debug, Clone, Copy)]
struct TurnBudget {
    max_tool_rounds: usize,
    max_duration_seconds: u64,
    max_requests: usize,
    max_tokens: u64,
}

impl TurnBudget {
    fn from_snapshot(value: &serde_json::Value) -> Self {
        if !kairos_platform_pent_runtime::budget_policy::budget_limits_enabled(value) {
            return Self {
                max_tool_rounds: usize::MAX,
                max_duration_seconds: u64::MAX,
                max_requests: usize::MAX,
                max_tokens: u64::MAX,
            };
        }
        let mut budget = Self {
            max_tool_rounds: budget_u64(value, &["max_tool_rounds", "maxToolRounds"], 50)
                .clamp(1, MAX_TOOL_ROUNDS as u64) as usize,
            max_duration_seconds: budget_u64(
                value,
                &[
                    "max_duration_seconds",
                    "maxDurationSeconds",
                    "max_time_seconds",
                ],
                MAX_TURN_RUNTIME_SECS,
            )
            .clamp(1, MAX_TURN_RUNTIME_SECS),
            max_requests: budget_u64(value, &["max_requests", "maxRequests"], 200).clamp(1, 10_000)
                as usize,
            max_tokens: budget_u64(value, &["max_tokens", "maxTokens"], 200_000)
                .clamp(1, 2_000_000),
        };
        if std::env::var("PENTA_ENV").as_deref() == Ok("development")
            && let Ok(max_tokens) = std::env::var("PENTA_DEV_MAX_TOKENS")
            && let Ok(max_tokens) = max_tokens.parse::<u64>()
        {
            budget.max_tokens = max_tokens.clamp(1, 10_000_000);
        }
        budget
    }

    fn loop_limits(self) -> AgentLoopLimits {
        AgentLoopLimits {
            max_rounds: self.max_tool_rounds,
            max_duration_seconds: self.max_duration_seconds,
            max_requests: self.max_requests,
            max_tokens: self.max_tokens,
            stall_limit: Some(MAX_CONSECUTIVE_NO_PROGRESS_RESULTS),
        }
    }
}

fn budget_u64(value: &serde_json::Value, keys: &[&str], default: u64) -> u64 {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
        .unwrap_or(default)
}

pub struct ConversationRuntime {
    repository: Arc<dyn ConversationRepository>,
    context_source: Arc<dyn ConversationContextSource>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
    model_gateway: Arc<dyn ConversationModelGateway>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    instruction_sink: Arc<dyn ConversationInstructionSink>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    finding_recorder: Arc<dyn FindingRecorder>,
    plan_store: Arc<dyn PlanStore>,
    role_resolver: Arc<dyn RoleResolver>,
    skill_store: Arc<dyn SkillStore>,
    enrichment: Arc<dyn RuntimeEnrichment>,
    agent_sessions: Arc<dyn AgentSessionFactory>,
    events: Arc<dyn ConversationEventSink>,
    nats: Option<crate::transport::mq::NatsHandle>,
    async_tools: super::async_tools::AsyncToolRegistry,
}

struct MainInlineToolExecutor<'a> {
    runtime: &'a ConversationRuntime,
    session_id: &'a str,
    context: &'a ConversationContext,
    agent_session: &'a Arc<dyn AgentSession>,
}

#[async_trait::async_trait]
impl InlineToolExecutor for MainInlineToolExecutor<'_> {
    fn supports(&self, _tool_name: &str) -> bool {
        true
    }

    async fn execute(
        &self,
        call: &RuntimeToolCall,
        _context: &ToolExecutionContext,
    ) -> RuntimeToolResult {
        self.runtime
            .execute_fast_tool(call, self.session_id, self.context, self.agent_session)
            .await
    }
}

struct MainAgentRunHooks<'a> {
    runtime: &'a ConversationRuntime,
    turn_id: &'a str,
    session_id: &'a str,
    context: &'a mut ConversationContext,
    intent: &'a ConversationIntent,
    orchestration: super::agent_pipeline::OrchestrationMode,
    agent_session: &'a Arc<dyn AgentSession>,
    pentest_memory: &'a mut PentestMemory,
    knowledge_cache: &'a mut KnowledgeCache,
    attack_graph: &'a AttackGraph,
    scan_progress: &'a mut super::scan_progress::ScanProgress,
    web_coverage: &'a mut super::web_coverage::WebCoverageLedger,
    evidence_ids: &'a mut Vec<String>,
    run_summary: &'a mut String,
    stall_terminal_prompted: &'a mut bool,
    loaded_skills: &'a mut HashSet<String>,
    executed_call_signatures: &'a mut HashSet<String>,
    web_session: &'a mut super::web_session::WebSessionState,
    finding_projection: &'a mut super::finding_projection::FindingProjectionState,
}

#[async_trait::async_trait]
impl AgentRunHooks for MainAgentRunHooks<'_> {
    type Output = ModelTurnOutput;

    async fn run_round(
        &mut self,
        round: usize,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
        runner.run_model_round(round, self).await
    }
}

#[async_trait::async_trait]
impl AgentRoundHooks for MainAgentRunHooks<'_> {
    type Output = ModelTurnOutput;

    async fn prepare_round(&mut self, round: usize) -> anyhow::Result<()> {
        let injection = prepare_injection(
            self.runtime.enrichment.as_ref(),
            self.pentest_memory,
            self.knowledge_cache,
            self.attack_graph,
        )
        .await;
        let rmos_context = build_injected_context(&injection);
        if !rmos_context.is_empty() {
            self.context.recent_messages.push(AgentMessage {
                message_id: format!("rmos_injection_{round}"),
                role: "system".into(),
                content: rmos_context,
                format: "text".into(),
            });
        }
        ContextManager::main().project(self.context);
        Ok(())
    }

    async fn generate(&mut self, _round: usize) -> anyhow::Result<ModelTurnOutput> {
        self.runtime
            .model_gateway
            .generate(self.context, self.intent)
            .await
    }

    fn is_transient_model_error(&self, error: &anyhow::Error) -> bool {
        super::agent_pipeline::is_transient_model_error(error)
    }

    async fn model_retry(
        &mut self,
        round: usize,
        error: &anyhow::Error,
        attempt: usize,
    ) -> anyhow::Result<()> {
        tracing::warn!(
            error = %error,
            attempt,
            "model call failed inside agent loop; retrying"
        );
        self.context.recent_messages.push(AgentMessage {
            message_id: format!("runtime_model_error:round_{round}"),
            role: "user".into(),
            content: format!(
                "运行时报告：上一次模型调用发生临时错误（{}）。\
                 请保持当前目标和已有 Evidence，继续执行；不要重复已经失败的策略。",
                compact_runtime_error(&error.to_string())
            ),
            format: "text".into(),
        });
        Ok(())
    }

    async fn model_exhausted(
        &mut self,
        _round: usize,
        error: &anyhow::Error,
        transient: bool,
        attempts: usize,
    ) -> anyhow::Result<()> {
        self.runtime
            .repository
            .fail_turn(
                self.turn_id,
                "MODEL_UNAVAILABLE",
                &error.to_string(),
                transient,
            )
            .await?;
        Err(anyhow::anyhow!("模型调用连续失败 {attempts} 次: {error}"))
    }

    async fn empty_model_retry(&mut self, round: usize, _attempt: usize) -> anyhow::Result<()> {
        self.context.recent_messages.push(AgentMessage {
            message_id: format!("runtime_empty_model_response:round_{round}"),
            role: "user".into(),
            content: "运行时报告：上一条模型响应为空，没有正文或有效工具调用。\
                      请保留当前目标和已有 Evidence，选择可用工具继续执行；\
                      若确实无法继续，请明确返回 ASK_USER 或 NO_PATH。"
                .into(),
            format: "text".into(),
        });
        Ok(())
    }

    async fn empty_model_exhausted(
        &mut self,
        _round: usize,
        attempts: usize,
    ) -> anyhow::Result<()> {
        self.runtime
            .repository
            .fail_turn(
                self.turn_id,
                "MODEL_EMPTY_RESPONSE",
                &format!("模型连续 {attempts} 次返回了空内容"),
                true,
            )
            .await?;
        Err(anyhow::anyhow!("模型连续 {attempts} 次返回了空内容"))
    }

    async fn handle_output(
        &mut self,
        round: usize,
        output: ModelTurnOutput,
        directive: Option<AgentDirective>,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
        self.process_output(round, output, directive, runner).await
    }
}

impl MainAgentRunHooks<'_> {
    async fn process_output(
        &mut self,
        round: usize,
        output: ModelTurnOutput,
        directive: Option<AgentDirective>,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<ModelTurnOutput>> {
        self.runtime
            .process_main_agent_output(
                round,
                runner,
                self.turn_id,
                self.session_id,
                self.context,
                self.orchestration,
                self.agent_session,
                self.pentest_memory,
                self.scan_progress,
                self.web_coverage,
                self.evidence_ids,
                self.run_summary,
                self.stall_terminal_prompted,
                self.loaded_skills,
                self.executed_call_signatures,
                self.web_session,
                self.finding_projection,
                output,
                directive,
            )
            .await
    }
}

impl ConversationRuntime {
    async fn execute_async_control_tool(
        &self,
        tc: &RuntimeToolCall,
        session_id: &str,
        context: &ConversationContext,
    ) -> RuntimeToolResult {
        let content = async {
            let run_id = context
                .run_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("execution 控制工具只能在已有 Run 中使用"))?;
            match tc.name.as_str() {
                "start_tool_execution" => {
                    let parsed = serde_json::from_str::<serde_json::Value>(&tc.arguments)
                        .map_err(|error| anyhow::anyhow!("启动参数不是有效 JSON: {error}"))?;
                    let tool_name = parsed
                        .get("tool_name")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| anyhow::anyhow!("start_tool_execution 缺少 tool_name"))?;
                    let arguments = parsed
                        .get("arguments")
                        .filter(|value| value.is_object())
                        .ok_or_else(|| {
                            anyhow::anyhow!("start_tool_execution.arguments 必须是对象")
                        })?
                        .to_string();
                    anyhow::ensure!(
                        super::tool_executor::has_conversation_handler(tool_name),
                        "工具 {tool_name} 当前不可执行或未注册"
                    );
                    anyhow::ensure!(
                        !is_runtime_reference_tool(tool_name),
                        "内联控制/查询工具不能作为后台安全任务启动"
                    );
                    if let Some(whitelist) = context
                        .policy_snapshot
                        .pointer("/_role/tool_whitelist")
                        .and_then(serde_json::Value::as_array)
                        .filter(|values| !values.is_empty())
                    {
                        anyhow::ensure!(
                            whitelist
                                .iter()
                                .any(|value| value.as_str() == Some(tool_name)),
                            "当前 Agent 角色不允许后台调用工具 {tool_name}"
                        );
                    }
                    match self
                        .approval_gate
                        .review(&ToolApprovalRequest {
                            run_id: run_id.to_string(),
                            session_id: session_id.to_string(),
                            tool_call_id: tc.id.clone(),
                            tool_name: tool_name.to_string(),
                            arguments: arguments.clone(),
                        })
                        .await?
                    {
                        ToolApprovalOutcome::Approved(effective_arguments) => {
                            self.async_tools
                                .start(
                                    self.repository.clone(),
                                    self.tool_executor.clone(),
                                    ToolExecutionContext {
                                        scope_snapshot: context.scope_snapshot.clone(),
                                        session_id: session_id.to_string(),
                                        run_id: Some(run_id.to_string()),
                                    },
                                    tool_name.to_string(),
                                    effective_arguments,
                                )
                                .await
                        }
                        ToolApprovalOutcome::Rejected(reason) => Ok(serde_json::json!({
                            "error": reason,
                            "blocked": true,
                            "retryable": false
                        })),
                    }
                }
                "get_tool_execution" => {
                    self.async_tools
                        .get(self.repository.as_ref(), run_id, &tc.arguments)
                        .await
                }
                "wait_tool_execution" => {
                    self.async_tools
                        .wait(self.repository.as_ref(), run_id, &tc.arguments)
                        .await
                }
                "cancel_tool_execution" => {
                    self.async_tools
                        .cancel(self.repository.as_ref(), run_id, &tc.arguments)
                        .await
                }
                _ => anyhow::bail!("未知 execution 控制工具: {}", tc.name),
            }
        }
        .await;
        RuntimeToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            success: content
                .as_ref()
                .is_ok_and(|value| value.get("error").is_none()),
            content: content
                .map(|value| value.to_string())
                .unwrap_or_else(|error| {
                    serde_json::json!({"error":error.to_string(),"retryable":false}).to_string()
                }),
        }
    }

    async fn process_main_agent_output(
        &self,
        round: usize,
        runner: &mut AgentRunner,
        turn_id: &str,
        session_id: &str,
        context: &mut ConversationContext,
        orchestration: super::agent_pipeline::OrchestrationMode,
        agent_session: &Arc<dyn AgentSession>,
        pentest_memory: &mut PentestMemory,
        scan_progress: &mut super::scan_progress::ScanProgress,
        web_coverage: &mut super::web_coverage::WebCoverageLedger,
        evidence_ids: &mut Vec<String>,
        run_summary: &mut String,
        stall_terminal_prompted: &mut bool,
        loaded_skills: &mut HashSet<String>,
        executed_call_signatures: &mut HashSet<String>,
        web_session: &mut super::web_session::WebSessionState,
        finding_projection: &mut super::finding_projection::FindingProjectionState,
        mut output: ModelTurnOutput,
        directive: Option<AgentDirective>,
    ) -> anyhow::Result<AgentRoundOutcome<ModelTurnOutput>> {
        if !output.tool_calls.is_empty() {
            if let Some(rejection) = skill_gate_rejection(loaded_skills, &output.tool_calls) {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_skill_gate:{round}"),
                    role: "user".into(),
                    content: rejection.into(),
                    format: "text".into(),
                });
                return Ok(AgentRoundOutcome::Continue);
            }
            for call in &mut output.tool_calls {
                if web_session.inject(call) {
                    tracing::debug!(tool = %call.name, "injected run-local Web authentication state");
                }
            }
            let duplicates =
                deduplicate_tool_calls(&mut output.tool_calls, executed_call_signatures);
            if output.tool_calls.is_empty() {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_duplicate_gate:{round}"),
                    role: "user".into(),
                    content: "本轮工具调用与同轮或本 Run 已成功的只读调用完全重复，已跳过执行。\
                              请复用已有 Evidence，改变目标路径、参数或验证方法。"
                        .into(),
                    format: "text".into(),
                });
                return Ok(AgentRoundOutcome::Continue);
            }
            if duplicates > 0 {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_duplicate_notice:{round}"),
                    role: "user".into(),
                    content: format!(
                        "运行时已跳过 {duplicates} 个完全重复的工具调用；其余调用继续执行。"
                    ),
                    format: "text".into(),
                });
            }
            // Every tool path, including inline tools and agent_spawn, crosses
            // the same DesRed-compatible HITL fence before execution.
            let plan_required = orchestration
                == super::agent_pipeline::OrchestrationMode::PlanExecute
                && if let Some(run_id) = context.run_id.as_deref() {
                    !self.plan_store.exists(run_id).await.unwrap_or(false)
                } else {
                    false
                };
            let plan_step_blocked = orchestration
                == super::agent_pipeline::OrchestrationMode::PlanExecute
                && !plan_required
                && if let Some(run_id) = context.run_id.as_deref() {
                    !self
                        .plan_store
                        .has_executable_step(run_id)
                        .await
                        .unwrap_or(false)
                } else {
                    true
                };
            let dispatch_plan = ToolDispatcher::new(self.approval_gate.as_ref())
                .prepare(
                    &mut output.tool_calls,
                    &ToolDispatchContext {
                        run_id: context.run_id.clone(),
                        session_id: session_id.to_string(),
                        policy: ToolDispatchPolicy {
                            agent_management_only: orchestration
                                == super::agent_pipeline::OrchestrationMode::Supervisor,
                            plan_required,
                            plan_step_blocked,
                            single_operational_action: orchestration
                                == super::agent_pipeline::OrchestrationMode::PlanExecute,
                        },
                    },
                )
                .await;
            for decision in &dispatch_plan.decisions {
                if let Some(reason) = decision.repair.as_deref() {
                    tracing::warn!(tool_call_id = %decision.tool_call_id, %reason, "repaired malformed model tool arguments");
                }
            }
            // Write assistant tool_calls message to DB so SSE can stream it
            let tool_calls_json: Vec<serde_json::Value> = output
                .tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments
                        }
                    })
                })
                .collect();

            // Persist the model's optional reasoning/plan together with
            // tool_calls so it remains part of the native tool transcript.
            let mut assistant_tool_message = serde_json::json!({
                "content": if output.content.trim().is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(output.content.clone())
                },
                "tool_calls": tool_calls_json
            });
            if let Some(reasoning) = output.reasoning_summary.as_deref() {
                assistant_tool_message["reasoning_content"] = reasoning.into();
            }
            self.repository
                .write_intermediate_message(
                    turn_id,
                    session_id,
                    "assistant",
                    &assistant_tool_message.to_string(),
                    "tool_calls",
                )
                .await?;

            context.recent_messages.push(AgentMessage {
                message_id: format!("assistant_tool:round_{}", round),
                role: "assistant".into(),
                content: assistant_tool_message.to_string(),
                format: "tool_calls".into(),
            });

            // ── Phase pushes: guide the model through a systematic workflow ──
            if round == 2 {
                context.recent_messages.push(AgentMessage {
                    message_id: "runtime_push_js".into(),
                    role: "user".into(),
                    content: "第一步存活确认已完成。请立即扩展 Web 攻击面：\
                                  用 hakrawler/katana 递归收集页面与 JS URL，再用 arjun/paramspider \
                                  补充隐藏参数；发现特定技术栈后调用 tool_search 检索专项能力。"
                        .into(),
                    format: "text".into(),
                });
            }
            if round == 3 {
                context.recent_messages.push(AgentMessage {
                    message_id: "runtime_push_vuln_scan".into(),
                    role: "user".into(),
                    content: "信息收集阶段基本完成。请立即切换策略：用专项自动扫描工具（漏洞扫描、\
                                  XSS 检测、SQL 注入检测、目录爆破）批量发现漏洞，\
                                  用浏览器工具验证交互式漏洞，不要再逐一手工测试。"
                        .into(),
                    format: "text".into(),
                });
            }

            let mut loop_diagnostic = None;

            // ── Phase 1: group tools by capability ──
            // Read/info tools → concurrent. Destructive/write tools → sequential.
            let allowed_calls = output
                .tool_calls
                .iter()
                .take_while(|_| runner.start_request().is_ok())
                .count();
            let inline_executor = MainInlineToolExecutor {
                runtime: self,
                session_id,
                context: &context,
                agent_session: &agent_session,
            };
            let execution_context = ToolExecutionContext {
                scope_snapshot: context.scope_snapshot.clone(),
                session_id: session_id.to_string(),
                run_id: context.run_id.clone(),
            };
            let plan_action =
                if orchestration == super::agent_pipeline::OrchestrationMode::PlanExecute {
                    if let Some(run_id) = context.run_id.as_deref() {
                        let action_call = output.tool_calls[..allowed_calls].iter().find(|call| {
                            !matches!(
                                call.name.as_str(),
                                "plan_create" | "plan_get" | "plan_update"
                            ) && dispatch_plan.blocked_reason(&call.id).is_none()
                        });
                        if let Some(call) = action_call {
                            self.plan_store
                                .begin_action(run_id, &call.name, &call.id)
                                .await?
                                .map(|node_id| (node_id, call.id.clone()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
            let tool_results = ToolExecutionCoordinator::from_env(self.tool_executor.clone())
                .execute(
                    &output.tool_calls[..allowed_calls],
                    &dispatch_plan,
                    &execution_context,
                    &inline_executor,
                )
                .await;

            // ── Track unreachable targets for diagnostic ──
            let mut unreachable_targets: Vec<String> = Vec::new();
            let mut plan_action_success = true;
            let mut plan_action_evidence = Vec::new();
            let mut plan_action_error: Option<String> = None;
            let mut plan_step_closed: Option<(String, String)> = None;

            // ── Phase 3: process all results (SSE + evidence + traffic + summary) ──
            for item in &tool_results {
                let result = &item.result;
                let call_id = &item.call_id;
                let tool_name = &item.tool_name;
                let processed = ToolResultProcessor::new(self.repository.as_ref())
                    .with_events(self.events.as_ref())
                    .with_nats(self.nats.as_ref())
                    .process(
                        item,
                        ToolResultProcessingContext {
                            run_id: context.run_id.as_deref(),
                            turn_id: Some(turn_id),
                            session_id,
                            message_id: format!("tool_result:{call_id}"),
                            persistence: ToolResultPersistence::MainEvidence,
                            emit_completion: true,
                            capture_http_traffic: true,
                        },
                    )
                    .await?;
                let detached = detached_tool_observation(&item.tool_name, &result.content);
                let succeeded = detached
                    .as_ref()
                    .map(|value| value.succeeded)
                    .unwrap_or(processed.succeeded);
                let observed_tool_name = detached
                    .as_ref()
                    .map(|value| value.tool_name.as_str())
                    .unwrap_or(item.tool_name.as_str());
                let observed_arguments = detached
                    .as_ref()
                    .map(|value| value.arguments.as_str())
                    .unwrap_or(item.arguments.as_str());
                let observed_content = detached
                    .as_ref()
                    .map(|value| value.content.as_str())
                    .unwrap_or(result.content.as_str());
                web_session.observe(observed_arguments, observed_content);
                scan_progress.record_tool_arguments(
                    observed_tool_name,
                    observed_arguments,
                    succeeded,
                );
                web_coverage.observe(
                    observed_tool_name,
                    observed_arguments,
                    observed_content,
                    succeeded,
                );
                if item.tool_name == "record_finding" {
                    scan_progress.record_finding_arguments(&item.arguments, succeeded);
                }
                if tool_name == "plan_update"
                    && succeeded
                    && let Some(closed) =
                        super::agent_pipeline::plan_terminal_update(&item.arguments)
                {
                    plan_step_closed = Some(closed);
                }
                if succeeded {
                    executed_call_signatures
                        .insert(tool_call_signature(observed_tool_name, observed_arguments));
                }
                if tool_name == "skill_load" && succeeded {
                    if let Some(name) = skill_name_from_arguments(&item.arguments) {
                        loaded_skills.insert(name);
                    }
                }
                if tool_name == "tool_search" && succeeded {
                    let unlocked = super::tool_catalog::tool_search_result_names(&result.content);
                    if !unlocked.is_empty() {
                        self.model_gateway.unlock_tools(unlocked);
                    }
                }
                if plan_action
                    .as_ref()
                    .is_some_and(|(_, call_id)| call_id == &item.call_id)
                {
                    plan_action_success = succeeded;
                    if !succeeded {
                        plan_action_error = Some(result.content.chars().take(500).collect());
                    }
                }
                // Detect unreachable targets so the model doesn't retry dead hosts
                if !succeeded && is_unreachable_error(&result.content) {
                    if let Some(target) = extract_target_from_content(&result.content) {
                        let host = target_host_from_url(&target);
                        if !unreachable_targets.contains(&host) {
                            unreachable_targets.push(host);
                        }
                    }
                }
                let effective_observation = processed.effective_observation;
                runner.record_tool_result(succeeded, effective_observation);
                if let Some(diagnostic) = scan_progress.record_tool_result(
                    observed_tool_name,
                    succeeded,
                    effective_observation,
                    observed_content,
                ) {
                    loop_diagnostic = Some(diagnostic);
                }
                if tool_name == "traffic_replay" && succeeded {
                    scan_progress.record_traffic_replay();
                }

                let evidence_id = processed.evidence_id.or_else(|| {
                    detached
                        .as_ref()
                        .and_then(|value| value.evidence_id.clone())
                });
                if let Some(eid) = evidence_id.as_ref() {
                    evidence_ids.push(eid.clone());
                    scan_progress.record_evidence_tool(&eid, observed_tool_name);
                    if plan_action
                        .as_ref()
                        .is_some_and(|(_, action_call_id)| action_call_id == call_id)
                    {
                        plan_action_evidence.push(eid.clone());
                    }
                    if succeeded && let Some(run_id) = context.run_id.as_deref() {
                        for finding in finding_projection.observe(
                            observed_tool_name,
                            observed_arguments,
                            observed_content,
                            eid,
                        ) {
                            match self
                                .finding_recorder
                                .record(run_id, &finding.to_string())
                                .await
                            {
                                Ok(recorded) => {
                                    scan_progress
                                        .record_finding_arguments(&finding.to_string(), true);
                                    tracing::info!(
                                        run_id,
                                        tool = %observed_tool_name,
                                        finding = %finding["title"],
                                        duplicate = recorded["duplicate"].as_bool().unwrap_or(false),
                                        "projected deterministic finding from evidence"
                                    );
                                }
                                Err(error) => tracing::warn!(
                                    run_id,
                                    tool = %observed_tool_name,
                                    %error,
                                    "failed to project deterministic finding"
                                ),
                            }
                        }
                    }
                }
                append_run_summary(
                    run_summary,
                    round + 1,
                    observed_tool_name,
                    succeeded,
                    evidence_id.as_deref(),
                    observed_content,
                );
                context.recent_messages.push(processed.model_message);
            }
            if let (Some(run_id), Some((node_id, _))) =
                (context.run_id.as_deref(), plan_action.as_ref())
            {
                self.plan_store
                    .finish_action(
                        run_id,
                        node_id,
                        plan_action_success,
                        &plan_action_evidence,
                        plan_action_error.as_deref(),
                    )
                    .await?;
            }
            if runner.stop_reason().is_some() {
                return Ok(AgentRoundOutcome::Continue);
            }
            let loop_checkpoint = runner.checkpoint();
            ContextManager::main().checkpoint(
                context,
                loop_checkpoint,
                run_summary,
                Some(format!("{:?}", orchestration).to_ascii_lowercase()),
                Some(chrono::Utc::now().to_rfc3339()),
            );

            // ── RMOS: update memory from tool results ──
            for item in &tool_results {
                let result = &item.result;
                let tool_name = &item.tool_name;
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result.content) {
                    pentest_memory.absorb_tool_result(tool_name, &parsed);
                }
            }
            // Generate next_actions for uncovered methodology items
            pentest_memory.ensure_coverage_actions();

            self.repository
                .checkpoint_context(turn_id, &context, &run_summary, &evidence_ids)
                .await?;

            if scan_progress.stall_exhausted() && !*stall_terminal_prompted {
                *stall_terminal_prompted = true;
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_stall_exhausted:round_{round}"),
                    role: "user".into(),
                    content: "运行时报告：连续多次工具调用失败且没有产生新 Evidence。\
                                  请判断失败原因并立即改变工具或参数；如果没有可行替代路径，\
                                  返回 NO_PATH 并说明具体阻塞，不要继续机械重试。"
                        .into(),
                    format: "text".into(),
                });
            }
            // Same-tool consecutive failures: force a strategy switch.
            for (index, hint) in scan_progress.take_switch_hints().into_iter().enumerate() {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_switch_strategy:round_{round}:{index}"),
                    role: "user".into(),
                    content: hint,
                    format: "text".into(),
                });
            }
            // Plan-Execute replan checkpoint: a closed or blocked step must be
            // followed by an explicit continue / adjust / re-plan decision
            // instead of silently rushing into the next step.
            if orchestration == super::agent_pipeline::OrchestrationMode::PlanExecute {
                if let Some((node_id, status)) = plan_step_closed.take() {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_replan_checkpoint:round_{round}"),
                        role: "user".into(),
                        content: format!(
                            "重规划检查点：计划步骤 {node_id} 已标记为 {status}。推进前必须完成一次显式重规划决策：\n\
                             1. 评估本步骤产出是否满足成功标准；新发现的端点/参数/指纹线索应固化为后续步骤（plan_create 重排或 plan_update 补充）。\n\
                             2. 明确三选一：继续执行下一步（说明剩余计划仍然成立的依据）/ plan_update 调整后续步骤 / plan_create 重排剩余计划。\n\
                             3. 若连续步骤无有效证据产出，优先重排计划而不是机械推进。"
                        ),
                        format: "text".into(),
                    });
                } else if plan_action.is_some() && !plan_action_success {
                    context.recent_messages.push(AgentMessage {
                        message_id: format!("runtime_replan_blocked:round_{round}"),
                        role: "user".into(),
                        content: "重规划检查点：当前计划步骤因工具执行失败被标记为 blocked。必须立即重规划：\
                                  plan_update 将本步骤置为 ready 并换工具/换参数重试、标记 failed 后推进、\
                                  或 plan_create 重排剩余计划。blocked 步骤悬置时普通工具会被编排闸门拒绝。"
                            .into(),
                        format: "text".into(),
                    });
                }
            }
            if let Some(diagnostic) = loop_diagnostic {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_diagnostic:round_{round}"),
                    role: "user".into(),
                    content: diagnostic,
                    format: "text".into(),
                });
            }
            if scan_progress.requires_endpoint_coverage()
                && let Some(instruction) = web_coverage.next_actions_instruction()
            {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_endpoint_coverage:round_{round}"),
                    role: "user".into(),
                    content: instruction,
                    format: "text".into(),
                });
            }
            // Tell model about unreachable targets so it doesn't waste rounds retrying
            for host in &unreachable_targets {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_unreachable:{}", host),
                    role: "user".into(),
                    content: format!(
                        "目标 {host} 不可达（连接被拒绝或超时），请**跳过**该目标，\
                             不要再次调用 http_request/http_probe 指向它。\
                             如果所有目标都不可达请输出 NO_PATH。"
                    ),
                    format: "text".into(),
                });
            }
            // Remove repeatedly-failing tools from model catalog so the
            // agent is forced to try alternatives instead of retrying.
            let exhausted = scan_progress.exhausted_tools().clone();
            if !exhausted.is_empty() {
                self.model_gateway.set_exhausted_tools(exhausted);
            }
            return Ok(AgentRoundOutcome::Continue);
        }

        // A tool-free response must declare, or be treated as, a lifecycle
        // action. The runtime validates the action against actual progress
        // and Evidence rather than forcing a fixed pentest phase template.
        match directive.expect("runtime supplies a directive for tool-free output") {
            AgentDirective::Replan(reason) => {
                context.recent_messages.push(AgentMessage {
                    message_id: format!("assistant_replan:round_{round}"),
                    role: "assistant".into(),
                    content: format!("REPLAN: {reason}"),
                    format: "text".into(),
                });
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_replan:round_{round}"),
                    role: "user".into(),
                    content: "请根据新计划直接选择并调用下一项最有信息增益的可用工具；\
                                  不要只输出另一份计划。"
                        .into(),
                    format: "text".into(),
                });
                return Ok(AgentRoundOutcome::Continue);
            }
            AgentDirective::NoPath(reason) => {
                if let Some(rejection) = scan_progress.no_path_rejection() {
                    push_gate_rejection(
                        context,
                        round,
                        &output.content,
                        &format!("NO_PATH 被拒绝：{rejection}"),
                    );
                    return Ok(AgentRoundOutcome::Continue);
                }
                output.content = format!(
                    "当前授权范围和可用能力下无法继续：{}",
                    nonempty_or(&reason, "模型未提供具体原因")
                );
            }
            AgentDirective::AskUser(question) => {
                // Autonomous mode: treat ASK_USER as a signal to replan.
                // The model has exhausted its current approach — push it to try
                // alternative tools or conclude with NO_PATH.
                let reason = nonempty_or(&question, "模型判断需要补充信息，但当前为全自主模式");
                context.recent_messages.push(AgentMessage {
                    message_id: format!("assistant_askuser_replan:round_{round}"),
                    role: "assistant".into(),
                    content: format!("ASK_USER: {reason}"),
                    format: "text".into(),
                });
                context.recent_messages.push(AgentMessage {
                    message_id: format!("runtime_askuser_replan:round_{round}"),
                    role: "user".into(),
                    content: format!(
                        "你的 ASK_USER（{reason}）在全自主模式下不可用。\
                             请改为：1) 换其他工具或策略继续验证；\
                             2) 如果确实无路可走，输出 NO_PATH 说明阻塞点。"
                    ),
                    format: "text".into(),
                });
                return Ok(AgentRoundOutcome::Continue);
            }
            AgentDirective::Final(candidate) | AgentDirective::Candidate(candidate) => {
                if let Some(run_id) = context.run_id.as_deref() {
                    let pending = self
                        .repository
                        .pending_background_tool_executions(run_id)
                        .await?;
                    if !pending.is_empty() {
                        push_gate_rejection(
                            context,
                            round,
                            &output.content,
                            &format!(
                                "FINAL 被拒绝：仍有后台工具执行未完成：{}。请调用 wait_tool_execution/get_tool_execution 收集结果，或在确有必要时取消。",
                                pending.join(", ")
                            ),
                        );
                        return Ok(AgentRoundOutcome::Continue);
                    }
                }
                let orchestration_rejection =
                    if super::agent_pipeline::multi_agent_enabled(&context.policy_snapshot) {
                        self.orchestration_completion_rejection(
                            orchestration,
                            context.run_id.as_deref(),
                            agent_session,
                        )
                        .await?
                    } else {
                        None
                    };
                if let Some(rejection) = orchestration_rejection {
                    push_gate_rejection(
                        context,
                        round,
                        &output.content,
                        &format!("FINAL 被拒绝：{rejection}"),
                    );
                    return Ok(AgentRoundOutcome::Continue);
                }
                if scan_progress.requires_endpoint_coverage()
                    && let Some(rejection) = web_coverage.final_rejection()
                {
                    push_gate_rejection(
                        context,
                        round,
                        &output.content,
                        &format!("FINAL 被拒绝：{rejection}"),
                    );
                    return Ok(AgentRoundOutcome::Continue);
                }
                if let Some(rejection) = scan_progress.final_rejection(&candidate, &evidence_ids) {
                    push_gate_rejection(
                        context,
                        round,
                        &output.content,
                        &format!("FINAL 被拒绝：{rejection}"),
                    );
                    return Ok(AgentRoundOutcome::Continue);
                }
                if let Some(rejection) = self
                    .claim_projection_rejection(
                        context.run_id.as_deref(),
                        &candidate,
                        &evidence_ids,
                    )
                    .await?
                {
                    push_gate_rejection(
                        context,
                        round,
                        &output.content,
                        &format!("FINAL 被拒绝：{rejection}"),
                    );
                    return Ok(AgentRoundOutcome::Continue);
                }
                if let Some(rejection) = scan_progress.maturity_rejection(&candidate, &evidence_ids)
                {
                    push_gate_rejection(
                        context,
                        round,
                        &output.content,
                        &format!("FINAL 被拒绝：{rejection}"),
                    );
                    return Ok(AgentRoundOutcome::Continue);
                }
                output.content = candidate;
                // ── RMOS: don't stop if there are high-priority next actions ──
                // Cap P1/P2 pending items at 3 to prevent infinite loops when
                // the target simply doesn't have those vulnerability types.
                let pending: Vec<_> = pentest_memory
                    .next_actions
                    .iter()
                    .filter(|a| a.priority <= 2)
                    .take(3)
                    .collect();
                if !pending.is_empty() {
                    let mut lines = vec![
                        "## ⚠ 渗透测试尚未完成\n".to_string(),
                        "以下高优先级任务未执行，请选择一个继续操作：".to_string(),
                    ];
                    for a in pending.iter().take(5) {
                        let tool = a.tool.as_deref().unwrap_or("manual");
                        lines.push(format!(
                            "- [P{}] {} — `{}` {}",
                            a.priority, a.action, tool, a.target
                        ));
                    }
                    lines.push("\n选择最优先的任务，直接调用工具继续渗透。".to_string());
                    push_gate_rejection(context, round, &output.content, &lines.join("\n"));
                    return Ok(AgentRoundOutcome::Continue);
                }
            }
        }
        Ok(AgentRoundOutcome::Complete(output))
    }

    async fn orchestration_completion_rejection(
        &self,
        orchestration: super::agent_pipeline::OrchestrationMode,
        run_id: Option<&str>,
        agent_session: &Arc<dyn AgentSession>,
    ) -> anyhow::Result<Option<String>> {
        match orchestration {
            super::agent_pipeline::OrchestrationMode::Deep => Ok(None),
            super::agent_pipeline::OrchestrationMode::PlanExecute => {
                let Some(run_id) = run_id else {
                    return Ok(Some(
                        "plan_execute 缺少可持久化的 Run；请先创建或绑定 Run，再调用 plan_create 建立执行计划"
                            .into(),
                    ));
                };
                let Some(progress) = self.plan_store.progress(run_id).await? else {
                    return Ok(Some("尚未创建执行计划，请先调用 plan_create".into()));
                };
                Ok(super::agent_pipeline::plan_completion_rejection(&progress))
            }
            super::agent_pipeline::OrchestrationMode::Supervisor => {
                let agents = agent_session.list().await;
                Ok(super::agent_pipeline::supervisor_completion_rejection(
                    &agents,
                ))
            }
        }
    }

    pub fn new(
        repository: Arc<dyn ConversationRepository>,
        context_source: Arc<dyn ConversationContextSource>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
        model_gateway: Arc<dyn ConversationModelGateway>,
        knowledge_source: Arc<dyn ConversationKnowledgeSource>,
        instruction_sink: Arc<dyn ConversationInstructionSink>,
        tool_executor: Arc<dyn ConversationToolExecutor>,
        approval_gate: Arc<dyn ToolApprovalGate>,
        finding_recorder: Arc<dyn FindingRecorder>,
        plan_store: Arc<dyn PlanStore>,
        role_resolver: Arc<dyn RoleResolver>,
        skill_store: Arc<dyn SkillStore>,
        enrichment: Arc<dyn RuntimeEnrichment>,
        agent_sessions: Arc<dyn AgentSessionFactory>,
        events: Arc<dyn ConversationEventSink>,
    ) -> Self {
        Self {
            repository,
            context_source,
            intent_detector,
            model_gateway,
            knowledge_source,
            instruction_sink,
            tool_executor,
            approval_gate,
            finding_recorder,
            plan_store,
            role_resolver,
            skill_store,
            enrichment,
            agent_sessions,
            events,
            nats: None,
            async_tools: super::async_tools::AsyncToolRegistry::global(),
        }
    }

    pub fn with_nats(mut self, nats: crate::transport::mq::NatsHandle) -> Self {
        self.nats = Some(nats);
        self
    }

    /// Create the turn and user message in the database, return (turn_id, run_id).
    /// The turn status is "thinking" — the caller is expected to call `execute_turn`
    /// in a background task.
    pub async fn begin_turn_only(
        &self,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<(String, Option<String>)> {
        let intent = ConversationIntent {
            kind: ConversationMessageType::Chat,
            confidence: 0.5,
            requires_run_instruction: false,
            requires_confirmation: false,
        };
        self.repository.begin_turn(request, &intent).await
    }

    /// Execute the tool loop for an existing turn. Updates turn status on completion.
    pub async fn execute_turn(
        &self,
        turn_id: String,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<()> {
        if let Err(error) = self.execute_turn_inner(&turn_id, request).await {
            if let Err(mark_error) = self
                .repository
                .fail_turn(&turn_id, "TURN_EXECUTION_FAILED", &error.to_string(), true)
                .await
            {
                tracing::error!(
                    turn_id,
                    error = %mark_error,
                    "failed to persist conversation turn failure"
                );
            }
            return Err(error);
        }
        Ok(())
    }

    async fn execute_turn_inner(
        &self,
        turn_id: &str,
        request: &AgentTurnRequest,
    ) -> anyhow::Result<()> {
        self.repository
            .ensure_execution_run(turn_id, &request.session_id)
            .await?;
        let context = self.context_source.build(request).await?;
        let intent = self
            .intent_detector
            .detect(request, context.run_id.is_some())
            .await?;
        let run_id = context.run_id.clone();

        self.emit_event(
            "conversation.turn.started",
            serde_json::json!({"turn_id": turn_id, "session_id": request.session_id}),
        )
        .await;
        // Write run event for the Run detail page
        if let Some(ref run_id) = run_id {
            self.repository
                .record_run_event(
                    run_id,
                    "run.turn_started",
                    &serde_json::json!({"turn_id": turn_id}),
                )
                .await?;
        }

        let output = self
            .run_tool_loop(turn_id, &request.session_id, context, &intent)
            .await?;

        let result = AgentTurnResult {
            turn_id: turn_id.to_owned(),
            session_id: request.session_id.clone(),
            run_id,
            status: ConversationTurnStatus::Completed,
            message: AgentMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".into(),
                content: output.clone(),
                format: "markdown".into(),
            },
            intent,
            plan_update: None,
            observations: vec![],
            findings: vec![],
            citations: vec![],
            usage: Default::default(),
            error: None,
            completed_at: Some(Utc::now()),
        };
        if let Err(errors) = kairos_platform_pent_core::conversation::validate_turn_result(&result)
        {
            let error = anyhow::anyhow!("模型输出校验失败: {}", errors.join(","));
            self.repository
                .fail_turn(&turn_id, "INVALID_MODEL_OUTPUT", &error.to_string(), true)
                .await?;
            return Err(error);
        }
        self.repository.complete_turn(&result).await?;
        self.events
            .emit(
                "conversation.message.completed",
                serde_json::to_value(&result)?,
            )
            .await?;

        // 交互式会话路径也要在 Run 完成后提取并落库已确认漏洞。
        // 自主 Worker 路径由 `execute_session_run` 负责提取，这里只兜底 UI 路径。
        if let Some(run_id) = result.run_id.as_deref() {
            match self
                .finding_recorder
                .reconcile(run_id, &request.session_id)
                .await
            {
                Ok(count) if count > 0 => {
                    if let Err(error) = self
                        .repository
                        .record_run_event(
                            run_id,
                            "run.findings.persisted",
                            &serde_json::json!({"count": count}),
                        )
                        .await
                    {
                        tracing::warn!(run_id, error = %error, "failed to record findings event");
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(run_id, error = %error, "vulnerability extraction failed");
                }
            }
        }

        Ok(())
    }

    pub async fn handle(&self, request: &AgentTurnRequest) -> anyhow::Result<AgentTurnResult> {
        let context = self.context_source.build(request).await?;
        let intent = self
            .intent_detector
            .detect(request, context.run_id.is_some())
            .await?;
        let (turn_id, run_id) = self.repository.begin_turn(request, &intent).await?;
        self.events
            .emit(
                "conversation.turn.created",
                serde_json::json!({"turn_id": turn_id, "session_id": request.session_id}),
            )
            .await?;

        if intent.requires_run_instruction {
            let active_run = match run_id.as_deref() {
                Some(active_run) => active_run,
                None => {
                    let error = anyhow::anyhow!("运行指令缺少 active run");
                    self.repository
                        .fail_turn(&turn_id, "RUN_NOT_ACTIVE", &error.to_string(), false)
                        .await?;
                    return Err(error);
                }
            };
            if let Err(error) = self
                .instruction_sink
                .submit(active_run, request, &intent)
                .await
            {
                self.repository
                    .fail_turn(&turn_id, "INSTRUCTION_REJECTED", &error.to_string(), false)
                    .await?;
                return Err(error);
            }
        }

        let final_content = self
            .run_tool_loop(&turn_id, &request.session_id, context, &intent)
            .await?;

        let output = ModelTurnOutput {
            content: final_content.clone(),
            reasoning_summary: None,
            plan_update: None,
            citations: vec![],
            usage: Default::default(),
            tool_calls: vec![],
        };
        let result = AgentTurnResult {
            turn_id,
            session_id: request.session_id.clone(),
            run_id,
            status: ConversationTurnStatus::Completed,
            message: AgentMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".into(),
                content: final_content,
                format: "markdown".into(),
            },
            intent,
            plan_update: output.plan_update,
            observations: vec![],
            findings: vec![],
            citations: output.citations,
            usage: Default::default(),
            error: None,
            completed_at: Some(Utc::now()),
        };
        if let Err(errors) = kairos_platform_pent_core::conversation::validate_turn_result(&result)
        {
            let error = anyhow::anyhow!("模型输出校验失败: {}", errors.join(","));
            self.repository
                .fail_turn(
                    &result.turn_id,
                    "INVALID_MODEL_OUTPUT",
                    &error.to_string(),
                    true,
                )
                .await?;
            return Err(error);
        }
        self.repository.complete_turn(&result).await?;
        self.events
            .emit(
                "conversation.message.completed",
                serde_json::to_value(&result)?,
            )
            .await?;
        Ok(result)
    }

    /// Run the tool-calling loop and return the final content.
    /// Intermediate results are written to the database as conversation messages
    /// so the SSE stream can relay them to the frontend.
    pub(crate) async fn run_tool_loop(
        &self,
        turn_id: &str,
        session_id: &str,
        mut context: ConversationContext,
        intent: &ConversationIntent,
    ) -> anyhow::Result<String> {
        ContextManager::main().initialize(&mut context, None);
        let mut budget = TurnBudget::from_snapshot(&context.budget_snapshot);
        if let Some(max_iterations) = context
            .policy_snapshot
            .pointer("/multi_agent/max_iterations")
            .and_then(serde_json::Value::as_u64)
        {
            budget.max_tool_rounds = (max_iterations as usize).clamp(1, 10_000);
        }
        let checkpoint = ContextManager::main().restore_checkpoint(&context);
        // Per-turn budget counts actual model API usage below. Historical
        // messages are windowed by the model gateway and are already included
        // in each provider-reported input_tokens value.
        let max_consecutive_model_errors = std::env::var("PENTA_MAX_CONSECUTIVE_MODEL_ERRORS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_CONSECUTIVE_MODEL_ERRORS)
            .clamp(1, 5);
        let mut runner = AgentRunner::from_checkpoint(
            AgentRunConfig::main(budget.loop_limits(), max_consecutive_model_errors),
            checkpoint.loop_state,
        );
        let mut evidence_ids = Vec::new();
        let mut run_summary = context.run_summary.clone();
        let mut stall_terminal_prompted = false;
        let mut loaded_skills = HashSet::new();
        let mut executed_call_signatures = HashSet::new();
        let mut web_session = super::web_session::WebSessionState::default();
        let mut finding_projection = super::finding_projection::FindingProjectionState::default();
        let mut web_coverage = super::web_coverage::WebCoverageLedger::default();

        let user_input = context
            .recent_messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let scan_goal = format!("{user_input}\n{}", context.objective_snapshot);
        let mut scan_progress = super::scan_progress::ScanProgress::new(&scan_goal);
        let agent_session = self.agent_sessions.create(
            std::env::var("PENTA_MAX_AGENTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
        );
        if let Some(run_id) = context.run_id.as_deref() {
            let recovered = agent_session.recover(run_id).await?;
            if recovered > 0 {
                tracing::info!(run_id, recovered, "recovered unfinished sub-agents");
            }
        }
        let orchestration =
            super::agent_pipeline::OrchestrationMode::from_policy(&context.policy_snapshot);
        let orchestration_instruction =
            if super::agent_pipeline::multi_agent_enabled(&context.policy_snapshot) {
                orchestration.instruction()
            } else {
                "多智能体已关闭：当前代理直接完成任务，不要尝试调用 agent_spawn/agent_collect。"
            };
        context.recent_messages.push(AgentMessage {
            message_id: "runtime_orchestration_mode".into(),
            role: "user".into(),
            content: orchestration_instruction.into(),
            format: "text".into(),
        });
        if let Some(run_id) = context.run_id.as_deref() {
            let history = self.repository.load_run_tool_history(run_id).await?;
            evidence_ids = history.evidence_ids;
            for target in history.finding_targets {
                scan_progress.record_finding_arguments(
                    &serde_json::json!({"target": target}).to_string(),
                    true,
                );
            }
            for historical in history.tool_calls {
                web_session.observe(&historical.arguments.to_string(), "{}");
                web_coverage.observe(
                    &historical.tool_name,
                    &historical.arguments.to_string(),
                    "{}",
                    historical.success,
                );
                if historical.tool_name == "skill_load" && historical.success {
                    if let Some(name) = skill_name_from_arguments(&historical.arguments.to_string())
                    {
                        loaded_skills.insert(name);
                    }
                }
                if historical.success {
                    executed_call_signatures.insert(tool_call_signature(
                        &historical.tool_name,
                        &historical.arguments.to_string(),
                    ));
                }
                scan_progress.record_tool_arguments(
                    &historical.tool_name,
                    &historical.arguments.to_string(),
                    historical.success,
                );
                let effective_observation =
                    historical.success && !is_reference_tool(&historical.tool_name);
                scan_progress.restore_tool_result(
                    &historical.tool_name,
                    historical.success,
                    effective_observation,
                    historical.semantic_fingerprint.as_deref(),
                );
            }
        }

        // Inject the autonomous loop prompt at the start of tool execution.
        // This reminds the model it is in autonomous mode and must directly
        // call tools or emit lifecycle actions — no waiting for approval.
        context.recent_messages.push(AgentMessage {
            message_id: "runtime_autonomous_loop_start".into(),
            role: "user".into(),
            content: AUTONOMOUS_LOOP_PROMPT.to_string(),
            format: "text".into(),
        });
        if let Some(instruction) = scan_progress.coverage_instruction() {
            context.recent_messages.push(AgentMessage {
                message_id: "runtime_coverage_gate".into(),
                role: "user".into(),
                content: instruction,
                format: "text".into(),
            });
        }

        // ── RMOS: Memory + Knowledge + Attack Graph ──────────────────
        let mut pentest_memory = PentestMemory::new(&scan_goal);
        let mut knowledge_cache = KnowledgeCache::new();
        let attack_graph = AttackGraph::new();

        // ── Planning phase (before tool execution) ──────────────────
        // When the input is complex, inject a planning prompt that
        // guides the model to think through the approach first.
        if user_input.chars().count() >= 60
            || user_input.contains("http://")
            || user_input.contains("https://")
            || user_input.chars().any(|c| c > '\u{2e80}')
        {
            let planning_instruction = format!(
                "\n\n## 攻击计划\n\
                 目标：{user_input}\n\n\
                 在调用工具之前，先用一段话描述你的攻击计划：\n\
                 1. 信息收集阶段用什么工具（nmap, subfinder, whatweb 等）\n\
                 2. 漏洞扫描阶段用什么工具（nuclei, sqlmap, dalfox 等）\n\
                 3. 验证阶段怎么确认漏洞\n\n\
                 然后立即开始执行第一步，直接调用工具。"
            );
            context.recent_messages.push(AgentMessage {
                message_id: "runtime_planning_prompt".into(),
                role: "user".into(),
                content: planning_instruction,
                format: "text".into(),
            });
        }

        let mut hooks = MainAgentRunHooks {
            runtime: self,
            turn_id,
            session_id,
            context: &mut context,
            intent,
            orchestration,
            agent_session: &agent_session,
            pentest_memory: &mut pentest_memory,
            knowledge_cache: &mut knowledge_cache,
            attack_graph: &attack_graph,
            scan_progress: &mut scan_progress,
            web_coverage: &mut web_coverage,
            evidence_ids: &mut evidence_ids,
            run_summary: &mut run_summary,
            stall_terminal_prompted: &mut stall_terminal_prompted,
            loaded_skills: &mut loaded_skills,
            executed_call_signatures: &mut executed_call_signatures,
            web_session: &mut web_session,
            finding_projection: &mut finding_projection,
        };
        let run_result = match runner.run(&mut hooks).await {
            Ok(result) => result,
            Err(error) => {
                drop(hooks);
                agent_session.abort_all().await;
                return Err(error);
            }
        };
        drop(hooks);
        let mut final_output = run_result.output;

        if !runner.finished_normally() {
            let stop_reason = runner
                .stop_reason()
                .map(|reason| reason.as_str())
                .unwrap_or("rounds");
            let loop_checkpoint = runner.checkpoint();
            context.recent_messages.push(AgentMessage {
                message_id: "runtime_finalization".into(),
                role: "user".into(),
                content: runtime_finalization_prompt(
                    stop_reason,
                    runner.rounds_used(),
                    loop_checkpoint.request_count,
                    loop_checkpoint.token_count,
                    &run_summary,
                    &evidence_ids,
                ),
                format: "text".into(),
            });
            match self.model_gateway.generate(&context, intent).await {
                Ok(output) if !output.content.trim().is_empty() => {
                    final_output = Some(ModelTurnOutput {
                        content: normalize_runtime_finalization(&output.content, stop_reason),
                        ..output
                    });
                }
                Ok(_) => {
                    tracing::warn!(stop_reason, "budget finalization returned empty content");
                }
                Err(error) => {
                    tracing::warn!(
                        stop_reason,
                        error = %error,
                        "budget finalization model call failed"
                    );
                }
            }
            if final_output.is_none() {
                final_output = Some(ModelTurnOutput {
                    content: fallback_runtime_summary(stop_reason, &run_summary, &evidence_ids),
                    reasoning_summary: None,
                    plan_update: None,
                    citations: vec![],
                    usage: Default::default(),
                    tool_calls: vec![],
                });
            }
        }
        let output = final_output.ok_or_else(|| anyhow::anyhow!("模型未返回有效响应"))?;
        let loop_checkpoint = runner.checkpoint();

        let final_content = if context.run_id.is_some() {
            let report = if output.content.trim().is_empty() {
                "本轮未生成文字结论；请依据 Evidence 进行复核。"
            } else {
                output.content.trim()
            };
            let run_reference = context.run_id.as_deref().unwrap_or("无");
            format!(
                "{report}\n\n---\n\n## 结构化证据索引\n\n\
                 - 工具调用：{}\n- 有效观察：{}\n\
                 - Evidence 数量：{}\n- Run：`{run_reference}`\n\n\
                 各项发现应引用其 Evidence；完整证据可在该 Run 的 Evidence 页面复核。\
                 原始响应未直接拼接进报告。",
                loop_checkpoint.request_count,
                loop_checkpoint.effective_observations,
                evidence_ids.len()
            )
        } else if !output.content.trim().is_empty() {
            output.content.clone()
        } else {
            format!(
                "本轮完成了 {} 次受控工具调用并形成 {} 条有效观察。\
                 原始响应已保存为 Evidence，报告不内联原始输出。",
                loop_checkpoint.request_count, loop_checkpoint.effective_observations
            )
        };

        // Clean up any still-running sub-agents before returning
        agent_session.abort_all().await;

        Ok(final_content)
    }

    /// Execute a fast inline tool (knowledge, skill, traffic, evidence) synchronously.
    async fn execute_fast_tool(
        &self,
        tc: &RuntimeToolCall,
        session_id: &str,
        context: &ConversationContext,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        match tc.name.as_str() {
            "knowledge_search" => {
                self.execute_knowledge_search(
                    &tc.id,
                    &tc.arguments,
                    session_id,
                    context.run_id.as_deref(),
                )
                .await
            }
            "vision_analyze" => {
                self.execute_vision_analyze(&tc.id, &tc.arguments, context.run_id.as_deref())
                    .await
            }
            "skill_list" | "skill_load" | "tool_search" => {
                let content = self
                    .skill_store
                    .execute(&tc.name, &tc.arguments)
                    .await
                    .map(|v| v.to_string());
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| {
                        serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            name if super::async_tools::is_control_tool(name) => {
                self.execute_async_control_tool(tc, session_id, context)
                    .await
            }
            "agent_spawn" => {
                self.execute_agent_spawn(&tc.id, &tc.arguments, session_id, context, agent_session)
                    .await
            }
            "agent_status" => {
                self.execute_agent_status(&tc.id, &tc.arguments, agent_session)
                    .await
            }
            "agent_collect" => {
                self.execute_agent_collect(&tc.id, &tc.arguments, agent_session)
                    .await
            }
            "agent_list" => self.execute_agent_list(&tc.id, agent_session).await,
            "plan_create" | "plan_get" | "plan_update" => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => {
                        self.plan_store
                            .execute(run_id, &tc.name, &tc.arguments)
                            .await
                    }
                    None => Err(anyhow::anyhow!("计划工具只能在已有 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content
                        .map(|value| value.to_string())
                        .unwrap_or_else(|error| {
                            serde_json::json!({"error":error.to_string(),"retryable":false})
                                .to_string()
                        }),
                }
            }
            "record_finding" => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => self
                        .finding_recorder
                        .record(run_id, &tc.arguments)
                        .await
                        .map(|value| value.to_string()),
                    None => Err(anyhow::anyhow!("record_finding 只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|error| {
                        serde_json::json!({"error":error.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            name if is_traffic_tool(name) => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => {
                        self.execute_traffic_tool(
                            run_id,
                            session_id,
                            name,
                            &tc.id,
                            &tc.arguments,
                            &context.scope_snapshot,
                        )
                        .await
                    }
                    None => Err(anyhow::anyhow!("流量工具只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| {
                        serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            name if is_evidence_tool(name) => {
                let content = match context.run_id.as_deref() {
                    Some(run_id) => self
                        .repository
                        .execute_evidence_tool(run_id, name, &tc.arguments)
                        .await
                        .map(|v| v.to_string()),
                    None => Err(anyhow::anyhow!("Evidence 工具只能在已有执行 Run 中使用")),
                };
                super::tool_executor::ConversationToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    success: content.is_ok(),
                    content: content.unwrap_or_else(|e| {
                        serde_json::json!({"error":e.to_string(),"retryable":false}).to_string()
                    }),
                }
            }
            _ => {
                self.tool_executor
                    .execute(
                        &RuntimeToolCall {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        },
                        &ToolExecutionContext {
                            scope_snapshot: context.scope_snapshot.clone(),
                            session_id: session_id.to_string(),
                            run_id: context.run_id.clone(),
                        },
                    )
                    .await
            }
        }
    }

    /// Emit event to SSE sink and NATS (if connected).
    async fn emit_event(&self, event: &str, payload: serde_json::Value) {
        self.events.emit(event, payload.clone()).await.ok();
        if let Some(ref nats) = self.nats {
            let bytes = serde_json::to_vec(&payload).unwrap_or_default();
            nats.publish(&format!("penta.conversation.{}", event), &bytes)
                .await;
        }
    }

    async fn execute_knowledge_search(
        &self,
        tool_call_id: &str,
        arguments: &str,
        session_id: &str,
        run_id: Option<&str>,
    ) -> super::tool_executor::ConversationToolResult {
        let parsed = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
        let query = parsed
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if query.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "knowledge_search".into(),
                content: serde_json::json!({"error":"query 不能为空"}).to_string(),
                success: false,
            };
        }
        // ── M3 + D-07: multi-query hybrid search + deterministic rerank ──
        let multi_query_max = std::env::var("PENTA_MULTI_QUERY_MAX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(4)
            .clamp(1, 8);
        let queries = crate::tooling::reranker::knowledge::expand_queries(query, multi_query_max);
        let mut guides: Vec<KnowledgeGuide> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for variant in &queries {
            if let Ok(mut result) = self.knowledge_source.search_guides(variant, 3).await {
                for guide in result.drain(..) {
                    let key = format!("{}|{}", guide.id, guide.title);
                    if seen.insert(key) {
                        guides.push(guide);
                    }
                }
            }
            // Vector recall follows the same expanded query set as lexical
            // recall. This closes the multi-query hybrid loop instead of
            // embedding only the original wording.
            if let Ok(memories) = self.enrichment.search_memories(variant, 3).await {
                for memory in memories {
                    let digest = hex::encode(sha2::Sha256::digest(memory.as_bytes()));
                    let key = format!("vector|{digest}");
                    if seen.insert(key) {
                        guides.push(KnowledgeGuide {
                            id: format!("vec-{}", &digest[..24]),
                            title: "向量记忆匹配".into(),
                            source: Some("pgvector".into()),
                            content: memory,
                        });
                    }
                }
            }
        }
        // Rerank by term overlap against the original query (title 3x body).
        guides.sort_by(|left, right| {
            crate::tooling::reranker::knowledge::score_guide(query, &right.title, &right.content)
                .partial_cmp(&crate::tooling::reranker::knowledge::score_guide(
                    query,
                    &left.title,
                    &left.content,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        guides.truncate(5);
        let success = !guides.is_empty();
        if success {
            self.log_knowledge_retrieval(query, &guides, session_id, run_id)
                .await;
        }
        super::tool_executor::ConversationToolResult {
            tool_call_id: tool_call_id.to_string(),
            tool_name: "knowledge_search".into(),
            content: serde_json::json!({
                "query": query,
                "guide_count": guides.len(),
                "guides": guides,
                "instruction": "根据 Wiki 方法和向量记忆选择当前可用工具继续验证。"
            })
            .to_string(),
            success,
        }
    }

    /// Vision analysis (D-10): analyze a screenshot/data URL with a vision
    /// model and return only the text summary. The image never enters the
    /// main agent context; the summary is persisted as evidence upstream.
    async fn execute_vision_analyze(
        &self,
        tool_call_id: &str,
        arguments: &str,
        _run_id: Option<&str>,
    ) -> super::tool_executor::ConversationToolResult {
        let parsed = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
        let image = parsed
            .get("image_data_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if image.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({"error": "image_data_url 不能为空"}).to_string(),
                success: false,
            };
        }
        if !image.starts_with("data:image/") {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({
                    "error": "image_data_url 必须是 data:image/...;base64, 数据 URL"
                })
                .to_string(),
                success: false,
            };
        }
        let question = parsed.get("question").and_then(serde_json::Value::as_str);
        match self.enrichment.analyze_image(image, question).await {
            Ok(summary) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({
                    "summary": summary,
                    "instruction": "视觉摘要仅作辅助线索，需用当前可用工具继续验证。"
                })
                .to_string(),
                success: true,
            },
            Err(error) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "vision_analyze".into(),
                content: serde_json::json!({"error": error.to_string(), "retryable": false})
                    .to_string(),
                success: false,
            },
        }
    }

    /// Best-effort audit log for knowledge retrieval; failures only warn.
    async fn log_knowledge_retrieval(
        &self,
        query: &str,
        guides: &[KnowledgeGuide],
        session_id: &str,
        run_id: Option<&str>,
    ) {
        let result = self
            .enrichment
            .log_knowledge_retrieval(query, guides, session_id, run_id)
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, query, "knowledge retrieval log write failed");
        }
    }

    /// Verify that quotable assertions in a verified FINAL are verbatim-
    /// projectable into the run's cited evidence. Returns the model-facing
    /// rejection reason when fabrication is detected.
    async fn claim_projection_rejection(
        &self,
        run_id: Option<&str>,
        candidate: &str,
        evidence_ids: &[String],
    ) -> anyhow::Result<Option<String>> {
        let Some(run_id) = run_id else {
            return Ok(None);
        };
        if !super::scan_progress::has_verified_claim(candidate) {
            return Ok(None);
        }
        let cited: Vec<String> = evidence_ids
            .iter()
            .filter(|id| candidate.contains(id.as_str()))
            .cloned()
            .collect();
        if cited.is_empty() {
            // The citation gate in `final_rejection` already rejects this case.
            return Ok(None);
        }
        let texts = self.repository.evidence_texts(run_id, &cited).await?;
        Ok(super::claim_gate::assess_claims(candidate, &texts).rejection())
    }

    // ── Agent parallelism tool handlers ────────────────────────────────

    async fn execute_agent_spawn(
        &self,
        tool_call_id: &str,
        arguments: &str,
        session_id: &str,
        context: &ConversationContext,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let args: serde_json::Value = match serde_json::from_str(arguments) {
            Ok(v) => v,
            Err(e) => {
                return super::tool_executor::ConversationToolResult {
                    tool_call_id: tool_call_id.to_string(),
                    tool_name: "agent_spawn".into(),
                    content: serde_json::json!({"error": format!("参数解析失败: {e}")}).to_string(),
                    success: false,
                };
            }
        };
        let description = args
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("未命名子任务")
            .to_string();
        let mut prompt = args
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&description)
            .to_string();
        // Validate against the caller-supplied prompt before any role prefix
        // is prepended, so a bare description cannot inherit substance from
        // the role instruction.
        let handoff_prompt = prompt.clone();
        let role = args
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if let Ok(Some(profile)) = self.role_resolver.resolve(role) {
            prompt = format!(
                "## 专家角色：{} ({})\n{}\n\n## 本次委派\n{}",
                profile.name, profile.id, profile.instruction, prompt
            );
        }
        if prompt.trim().is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({"error": "prompt 不能为空"}).to_string(),
                success: false,
            };
        }

        let target = args
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        if let Some(rejection) = super::agent_pipeline::delegation_handoff_rejection(
            &description,
            &handoff_prompt,
            target.as_deref(),
        ) {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({"error": rejection}).to_string(),
                success: false,
            };
        }

        let active = agent_session.active_count().await;
        let max_concurrent = std::env::var("PENTA_MAX_AGENTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4usize);
        if active >= max_concurrent {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({
                    "error": format!("已达到最大并发子代理数 ({})，当前活跃 {} 个，请等待部分完成后再创建", max_concurrent, active),
                    "active_agents": active,
                    "max_concurrent": max_concurrent,
                })
                .to_string(),
                success: false,
            };
        }

        match agent_session
            .spawn(AgentSpawnRequest {
                description: description.clone(),
                prompt: prompt.clone(),
                session_id: session_id.to_string(),
                parent_run_id: context.run_id.clone().unwrap_or_else(|| "unknown".into()),
                target,
                child_budget: inherit_child_budget(
                    context,
                    AgentChildBudget {
                        max_rounds: 12,
                        max_duration_seconds: 15 * 60,
                        max_requests: 80,
                        max_tokens: 100_000,
                        stall_limit: 4,
                    },
                ),
            })
            .await
        {
            Ok(agent_id) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({
                    "agent_id": agent_id,
                    "description": description,
                    "status": "running",
                    "active_agents": active + 1,
                    "max_concurrent": max_concurrent,
                    "hint": format!("子代理 {} 已启动，稍后使用 agent_collect 获取结果", agent_id),
                })
                .to_string(),
                success: true,
            },
            Err(e) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_spawn".into(),
                content: serde_json::json!({"error": e.to_string()}).to_string(),
                success: false,
            },
        }
    }

    async fn execute_agent_status(
        &self,
        tool_call_id: &str,
        arguments: &str,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let agent_id = args
            .get("agent_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        if agent_id.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_status".into(),
                content: serde_json::json!({"error": "agent_id 不能为空"}).to_string(),
                success: false,
            };
        }

        match agent_session.status(agent_id).await {
            Some(agent) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_status".into(),
                content: serde_json::to_value(&agent).unwrap_or_default().to_string(),
                success: true,
            },
            None => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_status".into(),
                content: serde_json::json!({"error": format!("子代理 {} 不存在", agent_id)})
                    .to_string(),
                success: false,
            },
        }
    }

    async fn execute_agent_collect(
        &self,
        tool_call_id: &str,
        arguments: &str,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
        let agent_id = args
            .get("agent_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        if agent_id.is_empty() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({"error": "agent_id 不能为空"}).to_string(),
                success: false,
            };
        }

        // Check if agent exists
        if agent_session.status(agent_id).await.is_none() {
            return super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({"error": format!("子代理 {} 不存在", agent_id)})
                    .to_string(),
                success: false,
            };
        }

        match agent_session.collect(agent_id).await {
            Some(result) => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({
                    "agent_id": agent_id,
                    "result": result,
                })
                .to_string(),
                success: !result.starts_with("子代理执行失败") && !result.starts_with("等待"),
            },
            None => super::tool_executor::ConversationToolResult {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "agent_collect".into(),
                content: serde_json::json!({
                    "agent_id": agent_id,
                    "error": "无法获取子代理结果",
                })
                .to_string(),
                success: false,
            },
        }
    }

    async fn execute_agent_list(
        &self,
        tool_call_id: &str,
        agent_session: &std::sync::Arc<dyn AgentSession>,
    ) -> super::tool_executor::ConversationToolResult {
        let agents = agent_session.list().await;
        super::tool_executor::ConversationToolResult {
            tool_call_id: tool_call_id.to_string(),
            tool_name: "agent_list".into(),
            content: serde_json::json!({
                "agents": agents,
                "count": agents.len(),
                "active": agents.iter().filter(|a| a.status == RuntimeAgentStatus::Running).count(),
            })
            .to_string(),
            success: true,
        }
    }

    /// Execute one of the traffic tools. Read-only tools delegate to the
    /// repository; `traffic_replay` re-issues the stored request through the
    /// isolated HTTP executor (scope guard applies) and records the new
    /// exchange linked to the original traffic row.
    async fn execute_traffic_tool(
        &self,
        run_id: &str,
        session_id: &str,
        tool_name: &str,
        tool_call_id: &str,
        arguments: &str,
        scope_snapshot: &serde_json::Value,
    ) -> anyhow::Result<String> {
        if tool_name != "traffic_replay" {
            return self
                .repository
                .execute_traffic_tool(run_id, tool_name, arguments)
                .await
                .map(|value| value.to_string());
        }
        let arguments_value = serde_json::from_str::<serde_json::Value>(arguments)?;
        let traffic_id = arguments_value
            .get("traffic_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("traffic_replay 缺少 traffic_id"))?;
        let replay_args = self
            .repository
            .load_traffic_request(run_id, traffic_id)
            .await?;
        let result = self
            .tool_executor
            .execute(
                &RuntimeToolCall {
                    id: tool_call_id.to_string(),
                    name: "http_request".into(),
                    arguments: replay_args.to_string(),
                },
                &ToolExecutionContext {
                    scope_snapshot: scope_snapshot.clone(),
                    session_id: session_id.to_string(),
                    run_id: Some(run_id.to_string()),
                },
            )
            .await;
        let traffic_ids = self
            .repository
            .record_http_traffic(
                run_id,
                tool_call_id,
                "http_request",
                &replay_args.to_string(),
                &result.content,
                Some(traffic_id),
            )
            .await?;
        let mut content = serde_json::from_str::<serde_json::Value>(&result.content)
            .unwrap_or_else(|_| serde_json::json!({"raw": result.content}));
        if let Some(object) = content.as_object_mut() {
            object.insert("replay_of".into(), serde_json::json!(traffic_id));
            object.insert("replay_traffic_ids".into(), serde_json::json!(traffic_ids));
        }
        Ok(content.to_string())
    }
}

fn append_run_summary(
    summary: &mut String,
    round: usize,
    tool_name: &str,
    success: bool,
    evidence_id: Option<&str>,
    observation: &str,
) {
    let observation = serde_json::from_str::<serde_json::Value>(observation)
        .ok()
        .map(|value| {
            for key in [
                "status",
                "status_code",
                "reachable",
                "url",
                "target",
                "findings",
                "endpoints",
                "security_hints",
            ] {
                if let Some(item) = value.get(key) {
                    return format!("{key}={}", compact_summary_value(item));
                }
            }
            "结构化观察已保存".to_string()
        })
        .unwrap_or_else(|| "非结构化观察已保存".to_string());
    let line = format!(
        "\n- 第{round}轮 `{tool_name}`：{}；{}；Evidence `{}`",
        if success { "成功" } else { "失败" },
        observation,
        evidence_id.unwrap_or("无")
    );
    summary.push_str(&line);
    if summary.chars().count() > MAX_RUN_SUMMARY_CHARS {
        *summary = summary
            .chars()
            .rev()
            .take(MAX_RUN_SUMMARY_CHARS)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        summary.insert_str(0, "[较早摘要已滚动压缩]\n");
    }
}

fn compact_summary_value(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 300 {
        text
    } else {
        format!("{}…", text.chars().take(300).collect::<String>())
    }
}

fn push_gate_rejection(
    context: &mut ConversationContext,
    round: usize,
    candidate: &str,
    rejection: &str,
) {
    context.recent_messages.push(AgentMessage {
        message_id: format!("assistant_gate_candidate:round_{round}"),
        role: "assistant".into(),
        content: candidate.to_string(),
        format: "text".into(),
    });
    context.recent_messages.push(AgentMessage {
        message_id: format!("runtime_gate:round_{round}"),
        role: "user".into(),
        content: format!(
            "自主循环完成闸门：{rejection}\n\
             请自行选择下一步：调用工具补充证据、REPLAN、ASK_USER，或在路径确实耗尽时\
             给出有依据的 NO_PATH。"
        ),
        format: "text".into(),
    });
}

fn runtime_finalization_prompt(
    stop_reason: &str,
    rounds: usize,
    requests: usize,
    tokens: u64,
    run_summary: &str,
    evidence_ids: &[String],
) -> String {
    format!(
        "## 运行时收尾\n\n\
         已触发 `{stop_reason}` 运行边界（轮次 {rounds}、请求 {requests}、累计 Token {tokens}）。\
         本次调用不再提供工具。请根据已有摘要和 Evidence 输出一次最终判断：\n\
         - 已有足够证据：输出 `FINAL:`，明确区分已验证事实与未验证线索，并引用 Evidence ID；\n\
         - 证据不足或路径耗尽：输出 `NO_PATH:`，说明阻塞和建议的后续动作；\n\
         - 必须由用户补充信息：输出 `ASK_USER:`。\n\n\
         不要请求或调用工具，不要声称任务完整完成。\n\n\
         ## 已有运行摘要\n{}\n\n## Evidence\n{}",
        nonempty_or(run_summary, "尚无可用运行摘要"),
        if evidence_ids.is_empty() {
            "无".to_string()
        } else {
            evidence_ids.join("、")
        }
    )
}

fn normalize_runtime_finalization(content: &str, stop_reason: &str) -> String {
    let conclusion = match parse_agent_directive(content) {
        AgentDirective::Final(value)
        | AgentDirective::NoPath(value)
        | AgentDirective::AskUser(value)
        | AgentDirective::Candidate(value)
        | AgentDirective::Replan(value) => value,
    };
    format!(
        "本次运行因 `{stop_reason}` 运行边界停止，以下为基于当前 Evidence 的阶段性结论：\n\n{}",
        nonempty_or(&conclusion, "模型未生成可用的阶段性结论")
    )
}

fn fallback_runtime_summary(
    stop_reason: &str,
    run_summary: &str,
    evidence_ids: &[String],
) -> String {
    format!(
        "本次运行因 `{stop_reason}` 运行边界停止，终局模型调用未返回可用内容。\
         以下内容仅代表已保存的阶段性进展，不表示测试完整完成。\n\n{}\n\nEvidence：{}",
        nonempty_or(run_summary, "尚无可用运行摘要。"),
        if evidence_ids.is_empty() {
            "无".to_string()
        } else {
            evidence_ids.join("、")
        }
    )
}

fn nonempty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
}

fn compact_runtime_error(error: &str) -> String {
    const MAX_ERROR_CHARS: usize = 500;
    let compact = error.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_ERROR_CHARS {
        compact
    } else {
        format!(
            "{}…",
            compact.chars().take(MAX_ERROR_CHARS).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentDirective, MAX_CONSECUTIVE_NO_PROGRESS_RESULTS, ModelToolCall, TurnBudget,
        compact_runtime_error, deduplicate_tool_calls, detached_tool_observation,
        parse_agent_directive, skill_gate_rejection, skill_name_from_arguments,
        tool_call_signature,
    };
    use kairos_platform_pent_api::ToolExecutionClass;
    use std::collections::HashSet;

    fn tool_call(name: &str) -> ModelToolCall {
        ModelToolCall {
            id: format!("call-{name}"),
            name: name.into(),
            arguments: "{}".into(),
            execution_class: ToolExecutionClass::ReadOnly,
        }
    }

    fn tool_call_with_arguments(name: &str, arguments: &str) -> ModelToolCall {
        ModelToolCall {
            id: format!("call-{name}"),
            name: name.into(),
            arguments: arguments.into(),
            execution_class: ToolExecutionClass::ReadOnly,
        }
    }

    #[test]
    fn unlimited_resource_budget_keeps_the_no_progress_liveness_guard() {
        let limits = TurnBudget::from_snapshot(&serde_json::json!({})).loop_limits();
        assert_eq!(limits.max_rounds, usize::MAX);
        assert_eq!(limits.max_duration_seconds, u64::MAX);
        assert_eq!(limits.max_requests, usize::MAX);
        assert_eq!(limits.max_tokens, u64::MAX);
        assert_eq!(
            limits.stall_limit,
            Some(MAX_CONSECUTIVE_NO_PROGRESS_RESULTS)
        );
    }

    #[test]
    fn operational_tools_require_a_successfully_loaded_skill() {
        let empty = HashSet::new();
        let generic = HashSet::from(["web-recon".to_string()]);
        assert!(skill_gate_rejection(&empty, &[tool_call("nmap_scan")]).is_some());
        assert!(skill_gate_rejection(&empty, &[tool_call("skill_list")]).is_none());
        assert!(skill_gate_rejection(&empty, &[tool_call("skill_load")]).is_none());
        assert!(skill_gate_rejection(&generic, &[tool_call("nmap_scan")]).is_none());
    }

    #[test]
    fn specialized_validation_requires_a_matching_skill_family() {
        let generic = HashSet::from(["web-recon".to_string()]);
        let xss = HashSet::from(["web-recon".to_string(), "xss-validation".to_string()]);
        assert!(skill_gate_rejection(&generic, &[tool_call("dalfox")]).is_some());
        assert!(skill_gate_rejection(&xss, &[tool_call("dalfox")]).is_none());
        assert!(skill_gate_rejection(&xss, &[tool_call("sqlmap")]).is_some());
    }

    #[test]
    fn skill_name_is_read_from_tool_arguments() {
        assert_eq!(
            skill_name_from_arguments(r#"{"name":"XSS-Validation"}"#).as_deref(),
            Some("xss-validation")
        );
    }

    #[test]
    fn duplicate_calls_in_one_batch_are_removed() {
        let call = tool_call_with_arguments("nuclei", r#"{"target":"http://dvwa"}"#);
        let mut calls = vec![call.clone(), call];
        assert_eq!(deduplicate_tool_calls(&mut calls, &HashSet::new()), 1);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn prior_get_is_removed_but_mutating_post_can_repeat() {
        let get = tool_call_with_arguments(
            "http-framework-test",
            r#"{"url":"http://dvwa/","method":"GET"}"#,
        );
        let post = tool_call_with_arguments(
            "http-framework-test",
            r#"{"url":"http://dvwa/weak_id/","method":"POST"}"#,
        );
        let executed = HashSet::from([tool_call_signature(&get.name, &get.arguments)]);
        let mut calls = vec![get, post];
        assert_eq!(deduplicate_tool_calls(&mut calls, &executed), 1);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].arguments.contains("POST"));
    }

    #[test]
    fn runtime_error_is_compacted_before_returning_to_model() {
        let error = format!("secret-like detail {}", "x".repeat(1_000));
        let compact = compact_runtime_error(&error);
        assert!(compact.chars().count() <= 501);
        assert!(compact.ends_with('…'));
    }

    #[test]
    fn parses_explicit_agent_lifecycle_actions() {
        assert_eq!(
            parse_agent_directive("FINAL: 结论"),
            AgentDirective::Final("结论".into())
        );
        assert_eq!(
            parse_agent_directive("ASK_USER: 请提供测试账号"),
            AgentDirective::AskUser("请提供测试账号".into())
        );
        assert_eq!(
            parse_agent_directive("NO_PATH: 缺少浏览器能力"),
            AgentDirective::NoPath("缺少浏览器能力".into())
        );
        assert_eq!(
            parse_agent_directive("REPLAN: 改用响应差异验证"),
            AgentDirective::Replan("改用响应差异验证".into())
        );
    }

    #[test]
    fn parses_json_action_protocol() {
        assert_eq!(
            parse_agent_directive(r#"{"action":"final","content":"目标已完成，证据 ev-1"}"#),
            AgentDirective::Final("目标已完成，证据 ev-1".into())
        );
        assert_eq!(
            parse_agent_directive(r#"{"action":"ASK_USER","content":"请提供目标 URL"}"#),
            AgentDirective::AskUser("请提供目标 URL".into())
        );
        assert_eq!(
            parse_agent_directive(r#"{"action":"no_path","content":"当前授权不支持该操作"}"#),
            AgentDirective::NoPath("当前授权不支持该操作".into())
        );
        assert_eq!(
            parse_agent_directive(r#"{"action":"replan","content":"改用目录扫描验证"}"#),
            AgentDirective::Replan("改用目录扫描验证".into())
        );
    }

    #[test]
    fn json_action_without_content_falls_back_to_raw() {
        assert_eq!(
            parse_agent_directive(r#"{"action":"final"}"#),
            AgentDirective::Final(r#"{"action":"final"}"#.to_string())
        );
    }

    #[test]
    fn plain_text_without_marker_is_candidate_final() {
        let directive = parse_agent_directive("目标似乎没有开放端口");
        assert_eq!(
            directive,
            AgentDirective::Candidate("目标似乎没有开放端口".into())
        );
    }

    #[test]
    fn chinese_markers_are_recognized() {
        assert_eq!(
            parse_agent_directive("最终结果：目标安全"),
            AgentDirective::Final("目标安全".into())
        );
        assert_eq!(
            parse_agent_directive("需要用户：请提供 API 密钥"),
            AgentDirective::AskUser("请提供 API 密钥".into())
        );
        assert_eq!(
            parse_agent_directive("无法继续：防火墙拦截所有请求"),
            AgentDirective::NoPath("防火墙拦截所有请求".into())
        );
        assert_eq!(
            parse_agent_directive("重新规划：改用 POST 请求测试"),
            AgentDirective::Replan("改用 POST 请求测试".into())
        );
    }

    #[test]
    fn completed_detached_result_is_attributed_to_the_underlying_tool() {
        let observation = detached_tool_observation(
            "wait_tool_execution",
            r#"{"status":"completed","tool_name":"nuclei","arguments":{"target":"https://example.test"},"evidence_id":"ev-1","lines":["finding-a","finding-b"]}"#,
        )
        .expect("completed execution should project an observation");

        assert_eq!(observation.tool_name, "nuclei");
        assert!(observation.arguments.contains("example.test"));
        assert_eq!(observation.content, "finding-a\nfinding-b");
        assert_eq!(observation.evidence_id.as_deref(), Some("ev-1"));
        assert!(observation.succeeded);
        assert!(
            detached_tool_observation(
                "get_tool_execution",
                r#"{"status":"running","tool_name":"nuclei"}"#,
            )
            .is_none()
        );
    }
}

fn is_skill_tool(tool_name: &str) -> bool {
    matches!(tool_name, "skill_list" | "skill_load")
}

fn deduplicate_tool_calls(calls: &mut Vec<ModelToolCall>, executed: &HashSet<String>) -> usize {
    let original_len = calls.len();
    let mut batch = HashSet::new();
    calls.retain(|call| {
        let signature = tool_call_signature(&call.name, &call.arguments);
        if !batch.insert(signature.clone()) {
            return false;
        }
        !(deduplicate_across_rounds(call) && executed.contains(&signature))
    });
    original_len - calls.len()
}

fn deduplicate_across_rounds(call: &ModelToolCall) -> bool {
    if call.name == "traffic_replay" || call.execution_class == ToolExecutionClass::Destructive {
        return false;
    }
    if matches!(call.name.as_str(), "http_request" | "http-framework-test") {
        let method = serde_json::from_str::<serde_json::Value>(&call.arguments)
            .ok()
            .and_then(|arguments| {
                arguments
                    .get("method")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_ascii_uppercase)
            })
            .unwrap_or_else(|| "GET".into());
        return matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS");
    }
    true
}

fn tool_call_signature(tool_name: &str, arguments: &str) -> String {
    let arguments = serde_json::from_str::<serde_json::Value>(arguments)
        .map(canonical_json)
        .unwrap_or_else(|_| serde_json::Value::String(arguments.trim().into()));
    format!(
        "{tool_name}:{:x}",
        sha2::Sha256::digest(arguments.to_string().as_bytes())
    )
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_json).collect())
        }
        serde_json::Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect::<std::collections::BTreeMap<_, _>>();
            serde_json::to_value(sorted).unwrap_or(serde_json::Value::Null)
        }
        value => value,
    }
}

fn skill_name_from_arguments(arguments: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .get("name")?
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.to_ascii_lowercase())
}

fn required_skill_family(tool_name: &str) -> Option<(&'static str, &'static [&'static str])> {
    match tool_name {
        "sqlmap" | "sqlmap_scan" | "sqli_validate" => Some(("SQL 注入", &["sql", "injection"])),
        "xss_validate" | "xsser" | "web_xss_scan" | "dalfox" => Some(("XSS", &["xss"])),
        "csrf_validate" => Some(("CSRF", &["csrf"])),
        "command_injection_validate" | "commix" => {
            Some(("命令注入", &["command-injection", "command_injection"]))
        }
        "file_inclusion_validate" => {
            Some(("文件包含", &["file-inclusion", "file_inclusion", "lfi"]))
        }
        "file_upload_validate" => Some(("文件上传", &["file-upload", "file_upload", "webshell"])),
        _ => None,
    }
}

fn skill_gate_rejection(
    loaded_skills: &HashSet<String>,
    calls: &[ModelToolCall],
) -> Option<String> {
    let has_operational_call = calls.iter().any(|call| {
        !matches!(
            call.name.as_str(),
            "skill_list"
                | "skill_load"
                | "tool_search"
                | "get_tool_execution"
                | "wait_tool_execution"
                | "cancel_tool_execution"
                | "knowledge_search"
                | "plan_create"
                | "plan_get"
                | "plan_update"
                | "agent_spawn"
                | "agent_status"
                | "agent_collect"
                | "agent_list"
        )
    });
    if loaded_skills.is_empty() && has_operational_call {
        return Some(
            "执行门槛：本 Run 尚未成功加载 Skill。请先单独调用 skill_list 查找匹配规程，\
             再单独调用 skill_load 加载至少一个 Skill；加载成功后才能执行侦察、扫描或验证工具。"
                .into(),
        );
    }
    for call in calls {
        let Some((family, keywords)) = required_skill_family(&call.name) else {
            continue;
        };
        let covered = loaded_skills
            .iter()
            .any(|skill| keywords.iter().any(|keyword| skill.contains(keyword)));
        if !covered {
            return Some(format!(
                "专项 Skill 门槛：调用 {} 前必须加载匹配的 {family} Skill。\
                 请用具体关键词调用 skill_list，再单独调用 skill_load；不要沿用不相关的通用 Skill。",
                call.name
            ));
        }
    }
    None
}

fn is_runtime_reference_tool(tool_name: &str) -> bool {
    super::async_tools::is_control_tool(tool_name)
        || is_evidence_tool(tool_name)
        || is_traffic_tool(tool_name)
        || is_finding_tool(tool_name)
        || matches!(
            tool_name,
            "knowledge_search"
                | "vision_analyze"
                | "skill_list"
                | "skill_load"
                | "tool_search"
                | "agent_spawn"
                | "agent_status"
                | "agent_collect"
                | "agent_list"
                | "plan_create"
                | "plan_get"
                | "plan_update"
        )
}

fn is_reference_tool(tool_name: &str) -> bool {
    tool_name == "knowledge_search"
        || tool_name == "tool_search"
        || is_evidence_tool(tool_name)
        || is_finding_tool(tool_name)
        || is_skill_tool(tool_name)
}

struct DetachedToolObservation {
    tool_name: String,
    arguments: String,
    content: String,
    evidence_id: Option<String>,
    succeeded: bool,
}

fn detached_tool_observation(
    control_tool_name: &str,
    content: &str,
) -> Option<DetachedToolObservation> {
    if !matches!(
        control_tool_name,
        "get_tool_execution" | "wait_tool_execution"
    ) {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(content).ok()?;
    let status = value.get("status").and_then(serde_json::Value::as_str)?;
    if status == "running" {
        return None;
    }
    let tool_name = value
        .get("tool_name")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let arguments = value
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}))
        .to_string();
    let content = value
        .get("lines")
        .and_then(serde_json::Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    Some(DetachedToolObservation {
        tool_name,
        arguments,
        content,
        evidence_id: value
            .get("evidence_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        succeeded: status == "completed",
    })
}

/// Detect "target unreachable" errors in tool output.
fn is_unreachable_error(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("could not connect")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("no route to host")
        || lower.contains("failed to connect")
        || lower.contains("curl: (7)")
        || lower.contains("curl: (6)")
        || lower.contains("curl: (28)")
        || lower.contains("\"exit_code\":7")
        || lower.contains("\"reachable\":false")
}

fn extract_target_from_content(content: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(url) = v.get("url").and_then(|v| v.as_str()) {
            return Some(url.to_string());
        }
        // Also check nested result
        if let Some(inner) = v.get("result").and_then(|v| v.as_str()) {
            return extract_target_from_content(inner);
        }
    }
    None
}

fn target_host_from_url(url: &str) -> String {
    url.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .split(':')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
}
