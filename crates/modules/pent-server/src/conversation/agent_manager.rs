//! Agent manager for concurrent sub-agent execution.
//!
//! Supports model-driven task decomposition: the model can call `agent_spawn`
//! to create sub-agents that run independent tool loops concurrently, then
//! `agent_collect` to gather results.

use std::sync::Arc;

use kairos_platform_pent_api::{
    AgentMessage, AgentTurnRequest, ConversationContext, ConversationIntent,
    ConversationMessageType,
};

use super::tool_result_processor::{
    ToolResultPersistence, ToolResultProcessingContext, ToolResultProcessor,
};
use super::{
    ConversationContextSource, ConversationIntentDetector, ConversationKnowledgeSource,
    ConversationModelGateway, ConversationRepository, ConversationToolExecutor, FindingRecorder,
    InlineToolExecutor, ModelTurnOutput, RuntimeToolCall, RuntimeToolResult, SkillStore,
    ToolApprovalGate, ToolExecutionContext,
};
use kairos_platform_pent_runtime::agent_loop::{AgentDirective, AgentLoopLimits, AgentLoopStop};
use kairos_platform_pent_runtime::agent_runner::{
    AgentRoundHooks, AgentRoundOutcome, AgentRunConfig, AgentRunHooks, AgentRunner,
    CompletionRejection,
};
use kairos_platform_pent_runtime::agent_scheduler::{
    AgentScheduler, AgentSchedulerEventKind, AgentSchedulerEventSink, AgentSchedulerPolicy,
    RecoveredAgent, ScheduledAgentExecutor,
};
use kairos_platform_pent_runtime::context_lifecycle::ContextManager;
use kairos_platform_pent_runtime::ports::{
    AgentChildBudget, AgentSession, AgentSessionFactory, AgentSpawnRequest, RuntimeSubAgent,
};
use kairos_platform_pent_runtime::tool_dispatch::{
    ToolDispatchContext, ToolDispatchPolicy, ToolDispatcher,
};
use kairos_platform_pent_runtime::tool_execution::ToolExecutionCoordinator;

pub struct ServerAgentSessionFactory {
    pool: kairos_platform_framework_database::PgPool,
    repository: Arc<dyn ConversationRepository>,
    gateway: Arc<dyn ConversationModelGateway>,
    context_source: Arc<dyn ConversationContextSource>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    finding_recorder: Arc<dyn FindingRecorder>,
    skill_store: Arc<dyn SkillStore>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
}

impl ServerAgentSessionFactory {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: kairos_platform_framework_database::PgPool,
        repository: Arc<dyn ConversationRepository>,
        gateway: Arc<dyn ConversationModelGateway>,
        context_source: Arc<dyn ConversationContextSource>,
        knowledge_source: Arc<dyn ConversationKnowledgeSource>,
        tool_executor: Arc<dyn ConversationToolExecutor>,
        approval_gate: Arc<dyn ToolApprovalGate>,
        finding_recorder: Arc<dyn FindingRecorder>,
        skill_store: Arc<dyn SkillStore>,
        intent_detector: Arc<dyn ConversationIntentDetector>,
    ) -> Self {
        Self {
            pool,
            repository,
            gateway,
            context_source,
            knowledge_source,
            tool_executor,
            approval_gate,
            finding_recorder,
            skill_store,
            intent_detector,
        }
    }
}

impl AgentSessionFactory for ServerAgentSessionFactory {
    fn create(&self, max_concurrent: usize) -> Arc<dyn AgentSession> {
        let lease_ttl_seconds = std::env::var("PENTA_SUB_AGENT_LEASE_TTL_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(90_u64)
            .clamp(15, 3600);
        let lease_ttl = std::time::Duration::from_secs(lease_ttl_seconds);
        let executor = Arc::new(ServerScheduledAgentExecutor {
            repository: self.repository.clone(),
            gateway: self.gateway.clone(),
            context_source: self.context_source.clone(),
            knowledge_source: self.knowledge_source.clone(),
            tool_executor: self.tool_executor.clone(),
            approval_gate: self.approval_gate.clone(),
            finding_recorder: self.finding_recorder.clone(),
            skill_store: self.skill_store.clone(),
            intent_detector: self.intent_detector.clone(),
        });
        let event_sink = Arc::new(ServerAgentSchedulerEventSink {
            pool: self.pool.clone(),
            claimant_id: uuid::Uuid::new_v4().to_string(),
            max_concurrent: max_concurrent.max(1),
            lease_ttl,
        });
        let mut policy = AgentSchedulerPolicy::new(max_concurrent);
        policy.execution_timeout = std::time::Duration::from_secs(
            std::env::var("PENTA_SUB_AGENT_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(15 * 60),
        );
        policy.collect_timeout = std::time::Duration::from_secs(
            std::env::var("PENTA_SUB_AGENT_COLLECT_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5 * 60),
        );
        let renew_seconds = std::env::var("PENTA_SUB_AGENT_LEASE_RENEW_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| (lease_ttl_seconds / 3).max(1))
            .clamp(1, (lease_ttl_seconds / 2).max(1));
        policy.claim_renew_interval = std::time::Duration::from_secs(renew_seconds);
        Arc::new(ServerAgentSession {
            scheduler: AgentScheduler::new(policy, executor, event_sink),
        })
    }
}

struct ServerAgentSession {
    scheduler: Arc<AgentScheduler>,
}

struct ServerScheduledAgentExecutor {
    repository: Arc<dyn ConversationRepository>,
    gateway: Arc<dyn ConversationModelGateway>,
    context_source: Arc<dyn ConversationContextSource>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    finding_recorder: Arc<dyn FindingRecorder>,
    skill_store: Arc<dyn SkillStore>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
}

#[async_trait::async_trait]
impl ScheduledAgentExecutor for ServerScheduledAgentExecutor {
    async fn execute(&self, agent_id: &str, request: AgentSpawnRequest) -> anyhow::Result<String> {
        execute_sub_agent(
            agent_id,
            &request.session_id,
            &request.prompt,
            request.target.as_deref(),
            &request.description,
            request.child_budget,
            self.repository.clone(),
            self.gateway.clone(),
            self.context_source.clone(),
            self.knowledge_source.clone(),
            self.tool_executor.clone(),
            self.approval_gate.clone(),
            self.finding_recorder.clone(),
            self.skill_store.clone(),
            self.intent_detector.clone(),
        )
        .await
    }
}

struct ServerAgentSchedulerEventSink {
    pool: kairos_platform_framework_database::PgPool,
    claimant_id: String,
    max_concurrent: usize,
    lease_ttl: std::time::Duration,
}

#[async_trait::async_trait]
impl AgentSchedulerEventSink for ServerAgentSchedulerEventSink {
    async fn emit(
        &self,
        parent_run_id: &str,
        kind: AgentSchedulerEventKind,
        agent: &RuntimeSubAgent,
        request: Option<&AgentSpawnRequest>,
    ) -> anyhow::Result<()> {
        let mut payload = serde_json::json!({
            "agent_id": agent.agent_id,
            "description": agent.description,
            "status": agent.status,
            "result": agent.result,
            "error": agent.error,
            "started_at": agent.started_at,
            "completed_at": agent.completed_at,
            "request": request,
        });
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 734291))")
            .bind(parent_run_id)
            .execute(&mut *tx)
            .await?;

        if kind == AgentSchedulerEventKind::Spawned {
            let active: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM pent_run_events spawned
                 WHERE spawned.run_id=$1 AND spawned.event_type='agent.spawned'
                   AND (spawned.payload->>'claim_expires_at')::timestamptz > now()
                   AND NOT EXISTS (
                     SELECT 1 FROM pent_run_events terminal
                     WHERE terminal.run_id=spawned.run_id
                       AND terminal.sequence>spawned.sequence
                       AND terminal.payload->>'agent_id'=spawned.payload->>'agent_id'
                       AND terminal.event_type IN ('agent.completed','agent.failed','agent.cancelled','agent.timed_out')
                   )",
            )
            .bind(parent_run_id)
            .fetch_one(&mut *tx)
            .await?;
            if active >= self.max_concurrent as i64 {
                anyhow::bail!("已达到跨实例最大并发子代理数 ({})", self.max_concurrent);
            }
            let expires_at = chrono::Utc::now()
                + chrono::Duration::from_std(self.lease_ttl)
                    .unwrap_or_else(|_| chrono::Duration::seconds(90));
            payload["claimant_id"] = serde_json::Value::String(self.claimant_id.clone());
            payload["claim_expires_at"] = serde_json::Value::String(expires_at.to_rfc3339());
        } else {
            let owns_claim: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1 FROM pent_run_events spawned
                    WHERE spawned.run_id=$1 AND spawned.event_type='agent.spawned'
                      AND spawned.payload->>'agent_id'=$2
                      AND spawned.payload->>'claimant_id'=$3
                      AND (spawned.payload->>'claim_expires_at')::timestamptz > now()
                      AND NOT EXISTS (
                        SELECT 1 FROM pent_run_events terminal
                        WHERE terminal.run_id=spawned.run_id
                          AND terminal.sequence>spawned.sequence
                          AND terminal.payload->>'agent_id'=spawned.payload->>'agent_id'
                          AND terminal.event_type IN ('agent.completed','agent.failed','agent.cancelled','agent.timed_out')
                      )
                )",
            )
            .bind(parent_run_id)
            .bind(&agent.agent_id)
            .bind(&self.claimant_id)
            .fetch_one(&mut *tx)
            .await?;
            if !owns_claim {
                tx.rollback().await?;
                return Ok(());
            }
        }

        sqlx::query(
            "INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload)
             SELECT $1,$2,COALESCE(max(sequence),0)+1,$3,now(),$4
             FROM pent_run_events WHERE run_id=$2",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(parent_run_id)
        .bind(kind.as_str())
        .bind(payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn load_unfinished(&self, parent_run_id: &str) -> anyhow::Result<Vec<RecoveredAgent>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 734291))")
            .bind(parent_run_id)
            .execute(&mut *tx)
            .await?;
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pent_run_events spawned
             WHERE spawned.run_id=$1 AND spawned.event_type='agent.spawned'
               AND (spawned.payload->>'claim_expires_at')::timestamptz > now()
               AND NOT EXISTS (
                 SELECT 1 FROM pent_run_events terminal
                 WHERE terminal.run_id=spawned.run_id
                   AND terminal.sequence>spawned.sequence
                   AND terminal.payload->>'agent_id'=spawned.payload->>'agent_id'
                   AND terminal.event_type IN ('agent.completed','agent.failed','agent.cancelled','agent.timed_out')
               )",
        )
        .bind(parent_run_id)
        .fetch_one(&mut *tx)
        .await?;
        let available = (self.max_concurrent as i64 - active).max(0);
        if available == 0 {
            tx.commit().await?;
            return Ok(Vec::new());
        }
        let rows: Vec<(i64, serde_json::Value)> = sqlx::query_as(
            "SELECT spawned.id,spawned.payload
             FROM pent_run_events spawned
             WHERE spawned.run_id=$1 AND spawned.event_type='agent.spawned'
               AND spawned.payload ? 'request'
               AND (NOT (spawned.payload ? 'claim_expires_at')
                    OR (spawned.payload->>'claim_expires_at')::timestamptz <= now())
               AND NOT EXISTS (
                 SELECT 1 FROM pent_run_events terminal
                 WHERE terminal.run_id=spawned.run_id
                   AND terminal.sequence > spawned.sequence
                   AND terminal.payload->>'agent_id'=spawned.payload->>'agent_id'
                   AND terminal.event_type IN ('agent.completed','agent.failed','agent.cancelled','agent.timed_out')
               )
             ORDER BY spawned.sequence
             FOR UPDATE OF spawned SKIP LOCKED
             LIMIT $2",
        )
        .bind(parent_run_id)
        .bind(available)
        .fetch_all(&mut *tx)
        .await?;
        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(self.lease_ttl)
                .unwrap_or_else(|_| chrono::Duration::seconds(90));
        let mut payloads = Vec::with_capacity(rows.len());
        for (id, mut payload) in rows {
            payload["claimant_id"] = serde_json::Value::String(self.claimant_id.clone());
            payload["claim_expires_at"] = serde_json::Value::String(expires_at.to_rfc3339());
            sqlx::query("UPDATE pent_run_events SET payload=$2 WHERE id=$1")
                .bind(id)
                .bind(&payload)
                .execute(&mut *tx)
                .await?;
            payloads.push(payload);
        }
        tx.commit().await?;
        payloads
            .into_iter()
            .map(|payload| {
                let state = RuntimeSubAgent {
                    agent_id: payload["agent_id"].as_str().unwrap_or_default().to_string(),
                    description: payload["description"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    status: kairos_platform_pent_runtime::ports::RuntimeAgentStatus::Running,
                    result: None,
                    error: None,
                    started_at: payload["started_at"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    completed_at: None,
                };
                let request = serde_json::from_value(payload["request"].clone())?;
                Ok(RecoveredAgent { state, request })
            })
            .collect()
    }

    async fn renew_claim(&self, parent_run_id: &str, agent_id: &str) -> anyhow::Result<bool> {
        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(self.lease_ttl)
                .unwrap_or_else(|_| chrono::Duration::seconds(90));
        let result = sqlx::query(
            "UPDATE pent_run_events spawned
             SET payload=spawned.payload || jsonb_build_object('claim_expires_at',$4::text)
             WHERE spawned.id=(
                 SELECT candidate.id FROM pent_run_events candidate
                 WHERE candidate.run_id=$1 AND candidate.event_type='agent.spawned'
                   AND candidate.payload->>'agent_id'=$2
                 ORDER BY candidate.sequence DESC LIMIT 1
             )
               AND spawned.payload->>'claimant_id'=$3
               AND (spawned.payload->>'claim_expires_at')::timestamptz > now()
               AND NOT EXISTS (
                 SELECT 1 FROM pent_run_events terminal
                 WHERE terminal.run_id=spawned.run_id
                   AND terminal.sequence>spawned.sequence
                   AND terminal.payload->>'agent_id'=spawned.payload->>'agent_id'
                   AND terminal.event_type IN ('agent.completed','agent.failed','agent.cancelled','agent.timed_out')
               )",
        )
        .bind(parent_run_id)
        .bind(agent_id)
        .bind(&self.claimant_id)
        .bind(expires_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[async_trait::async_trait]
impl AgentSession for ServerAgentSession {
    async fn recover(&self, parent_run_id: &str) -> anyhow::Result<usize> {
        self.scheduler.recover(parent_run_id).await
    }
    async fn active_count(&self) -> usize {
        self.scheduler.active_count().await
    }

    async fn spawn(&self, request: AgentSpawnRequest) -> anyhow::Result<String> {
        self.scheduler.spawn(request).await
    }

    async fn status(&self, agent_id: &str) -> Option<RuntimeSubAgent> {
        self.scheduler.status(agent_id).await
    }

    async fn collect(&self, agent_id: &str) -> Option<String> {
        self.scheduler.collect(agent_id).await
    }

    async fn list(&self) -> Vec<RuntimeSubAgent> {
        self.scheduler.list().await
    }

    async fn abort_all(&self) {
        self.scheduler.abort_all().await;
    }
}

// ── Sub-agent execution ─────────────────────────────────────────────────

async fn execute_sub_agent_inline_tool(
    tool_call_id: &str,
    tool_name: &str,
    arguments: &str,
    run_id: &str,
    repository: &Arc<dyn ConversationRepository>,
    knowledge_source: &Arc<dyn ConversationKnowledgeSource>,
    finding_recorder: &Arc<dyn FindingRecorder>,
    skill_store: &Arc<dyn SkillStore>,
) -> super::tool_executor::ConversationToolResult {
    let content = match tool_name {
        "knowledge_search" => {
            let parsed = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
            let query = parsed
                .get("query")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim();
            if query.is_empty() {
                Err(anyhow::anyhow!("query 不能为空"))
            } else {
                knowledge_source
                    .search_guides(query, 5)
                    .await
                    .map(|guides| serde_json::json!({"query":query,"guides":guides}))
            }
        }
        "evidence_list" | "evidence_search" | "evidence_view" => {
            repository
                .execute_evidence_tool(run_id, tool_name, arguments)
                .await
        }
        "skill_list" | "skill_load" => skill_store.execute(tool_name, arguments).await,
        "record_finding" => finding_recorder.record(run_id, arguments).await,
        _ => Err(anyhow::anyhow!("未知的子代理内联工具: {tool_name}")),
    };
    super::tool_executor::ConversationToolResult {
        tool_call_id: tool_call_id.to_string(),
        tool_name: tool_name.to_string(),
        success: content.is_ok(),
        content: content
            .map(|value| value.to_string())
            .unwrap_or_else(|error| {
                serde_json::json!({"error":error.to_string(),"retryable":false}).to_string()
            }),
    }
}

struct SubAgentInlineToolExecutor<'a> {
    run_id: &'a str,
    repository: &'a Arc<dyn ConversationRepository>,
    knowledge_source: &'a Arc<dyn ConversationKnowledgeSource>,
    finding_recorder: &'a Arc<dyn FindingRecorder>,
    skill_store: &'a Arc<dyn SkillStore>,
}

#[async_trait::async_trait]
impl InlineToolExecutor for SubAgentInlineToolExecutor<'_> {
    fn supports(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "knowledge_search"
                | "evidence_list"
                | "evidence_search"
                | "evidence_view"
                | "skill_list"
                | "skill_load"
                | "record_finding"
                | "agent_spawn"
                | "agent_status"
                | "agent_collect"
                | "agent_list"
        )
    }

    async fn execute(
        &self,
        call: &RuntimeToolCall,
        _context: &ToolExecutionContext,
    ) -> RuntimeToolResult {
        if matches!(
            call.name.as_str(),
            "agent_spawn" | "agent_status" | "agent_collect" | "agent_list"
        ) {
            return RuntimeToolResult {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                content: serde_json::json!({
                    "error": "子代理不允许调用智能体管理工具",
                    "blocked": true
                })
                .to_string(),
                success: false,
            };
        }
        execute_sub_agent_inline_tool(
            &call.id,
            &call.name,
            &call.arguments,
            self.run_id,
            self.repository,
            self.knowledge_source,
            self.finding_recorder,
            self.skill_store,
        )
        .await
    }
}

struct SubAgentRunHooks<'a> {
    agent_id: &'a str,
    session_id: &'a str,
    run_id: &'a str,
    context: &'a mut ConversationContext,
    intent: &'a ConversationIntent,
    repository: &'a Arc<dyn ConversationRepository>,
    gateway: &'a Arc<dyn ConversationModelGateway>,
    knowledge_source: &'a Arc<dyn ConversationKnowledgeSource>,
    tool_executor: &'a Arc<dyn ConversationToolExecutor>,
    approval_gate: &'a Arc<dyn ToolApprovalGate>,
    finding_recorder: &'a Arc<dyn FindingRecorder>,
    skill_store: &'a Arc<dyn SkillStore>,
}

#[async_trait::async_trait]
impl AgentRunHooks for SubAgentRunHooks<'_> {
    type Output = String;

    async fn run_round(
        &mut self,
        round: usize,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
        runner.run_model_round(round, self).await
    }

    async fn stopped(
        &mut self,
        reason: AgentLoopStop,
        checkpoint: kairos_platform_pent_runtime::agent_loop::AgentLoopCheckpoint,
        runner: &AgentRunner,
    ) -> anyhow::Result<Option<Self::Output>> {
        let result = match reason {
            AgentLoopStop::Requests => format!(
                "子代理请求数耗尽（{}），未完成子任务",
                runner.limits().max_requests
            ),
            AgentLoopStop::Stalled => format!(
                "子代理连续 {} 次工具失败，判定为阻塞",
                checkpoint.consecutive_failures
            ),
            other => format!("子代理因 {} 预算停止，未完成子任务", other.as_str()),
        };
        Ok(Some(result))
    }
}

#[async_trait::async_trait]
impl AgentRoundHooks for SubAgentRunHooks<'_> {
    type Output = String;

    async fn prepare_round(&mut self, _round: usize) -> anyhow::Result<()> {
        ContextManager::sub_agent().project(self.context);
        Ok(())
    }

    async fn generate(&mut self, _round: usize) -> anyhow::Result<ModelTurnOutput> {
        self.gateway.generate(self.context, self.intent).await
    }

    async fn model_exhausted(
        &mut self,
        _round: usize,
        error: &anyhow::Error,
        _transient: bool,
        attempts: usize,
    ) -> anyhow::Result<()> {
        tracing::warn!(agent_id = self.agent_id, %error, "sub-agent model call failed");
        Err(anyhow::anyhow!(
            "子代理模型调用连续失败 {attempts} 次: {error}"
        ))
    }

    async fn empty_model_retry(&mut self, round: usize, _attempt: usize) -> anyhow::Result<()> {
        self.context.recent_messages.push(AgentMessage {
            message_id: format!("sub_empty_model_response:{}:{round}", self.agent_id),
            role: "user".into(),
            content: "上一条模型响应为空。请直接调用工具继续验证，或明确返回 NO_PATH。".into(),
            format: "text".into(),
        });
        Ok(())
    }

    async fn empty_model_exhausted(
        &mut self,
        _round: usize,
        attempts: usize,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("子代理模型连续 {attempts} 次返回了空内容"))
    }

    async fn handle_output(
        &mut self,
        round: usize,
        output: ModelTurnOutput,
        directive: Option<AgentDirective>,
        runner: &mut AgentRunner,
    ) -> anyhow::Result<AgentRoundOutcome<Self::Output>> {
        if !output.tool_calls.is_empty() {
            let mut output = output;
            let dispatch_plan = ToolDispatcher::new(self.approval_gate.as_ref())
                .prepare(
                    &mut output.tool_calls,
                    &ToolDispatchContext {
                        run_id: Some(self.run_id.to_string()),
                        session_id: self.session_id.to_string(),
                        policy: ToolDispatchPolicy::default(),
                    },
                )
                .await;
            for decision in &dispatch_plan.decisions {
                if let Some(reason) = decision.repair.as_deref() {
                    tracing::warn!(
                        tool_call_id = %decision.tool_call_id,
                        %reason,
                        "repaired malformed sub-agent tool arguments"
                    );
                }
            }
            self.context.recent_messages.push(AgentMessage {
                message_id: format!("sub_assistant:{}:{round}", self.agent_id),
                role: "assistant".into(),
                content: serde_json::json!({
                    "content": output.content,
                    "tool_calls": output.tool_calls.iter().map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {"name": tc.name, "arguments": tc.arguments}
                        })
                    }).collect::<Vec<_>>()
                })
                .to_string(),
                format: "tool_calls".into(),
            });

            let allowed_calls = output
                .tool_calls
                .iter()
                .take_while(|_| runner.start_request().is_ok())
                .count();
            let inline_executor = SubAgentInlineToolExecutor {
                run_id: self.run_id,
                repository: self.repository,
                knowledge_source: self.knowledge_source,
                finding_recorder: self.finding_recorder,
                skill_store: self.skill_store,
            };
            let execution_context = ToolExecutionContext {
                scope_snapshot: self.context.scope_snapshot.clone(),
                session_id: self.session_id.to_string(),
                run_id: Some(self.run_id.to_string()),
            };
            let tool_results = ToolExecutionCoordinator::from_env(self.tool_executor.clone())
                .execute(
                    &output.tool_calls[..allowed_calls],
                    &dispatch_plan,
                    &execution_context,
                    &inline_executor,
                )
                .await;

            for item in &tool_results {
                let call_id = &item.call_id;
                let processed = ToolResultProcessor::new(self.repository.as_ref())
                    .process(
                        item,
                        ToolResultProcessingContext {
                            run_id: Some(self.run_id),
                            turn_id: None,
                            session_id: self.session_id,
                            message_id: format!("sub_tool:{}:{call_id}", self.agent_id),
                            persistence: ToolResultPersistence::ExternalBestEffort,
                            emit_completion: false,
                            capture_http_traffic: false,
                        },
                    )
                    .await?;
                runner.record_tool_result(processed.succeeded, processed.effective_observation);
                self.context.recent_messages.push(processed.model_message);
            }
            return Ok(AgentRoundOutcome::Continue);
        }

        let loop_checkpoint = runner.checkpoint();
        match directive.expect("runtime supplies a directive for tool-free output") {
            AgentDirective::Final(text) => {
                let directive = AgentDirective::Final(text.clone());
                if let Some(rejection) = runner.completion_rejection(&directive) {
                    let content = match rejection {
                        CompletionRejection::InsufficientToolCalls { required, actual } => format!(
                            "FINAL 被拒绝：工具调用不足（当前 {actual} 次，需求最少 {required} 次）。\
                             请至少调用一个工具收集证据后再提交 FINAL。"
                        ),
                        CompletionRejection::NoEffectiveObservation =>
                            "FINAL 被拒绝：所有工具调用均失败或无有效观察。请改变策略重试，或使用 NO_PATH 报告阻塞。".into(),
                        CompletionRejection::ExplicitFinalRequired =>
                            "FINAL 被拒绝：必须显式声明 FINAL。".into(),
                    };
                    self.context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_gate_reject:{}", self.agent_id),
                        role: "user".into(),
                        content,
                        format: "text".into(),
                    });
                    return Ok(AgentRoundOutcome::Continue);
                }
                Ok(AgentRoundOutcome::Complete(text))
            }
            AgentDirective::Replan(reason) => {
                self.context.recent_messages.push(AgentMessage {
                    message_id: format!("sub_replan:{}:{round}", self.agent_id),
                    role: "assistant".into(),
                    content: format!("REPLAN: {reason}"),
                    format: "text".into(),
                });
                self.context.recent_messages.push(AgentMessage {
                    message_id: format!("sub_replan_prompt:{}", self.agent_id),
                    role: "user".into(),
                    content: "请根据新计划直接选择并调用工具，不要只输出计划。".into(),
                    format: "text".into(),
                });
                Ok(AgentRoundOutcome::Continue)
            }
            AgentDirective::NoPath(reason) => {
                if loop_checkpoint.tool_call_count == 0 {
                    self.context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_no_path_reject:{}", self.agent_id),
                        role: "user".into(),
                        content: "NO_PATH 被拒绝：尚未调用任何工具，不能判定无可行路径。请先尝试调用工具。".into(),
                        format: "text".into(),
                    });
                    Ok(AgentRoundOutcome::Continue)
                } else {
                    Ok(AgentRoundOutcome::Complete(format!("NO_PATH: {reason}")))
                }
            }
            AgentDirective::AskUser(question) => {
                Ok(AgentRoundOutcome::Complete(format!("ASK_USER: {question}")))
            }
            AgentDirective::Candidate(text) => {
                let directive = AgentDirective::Candidate(text.clone());
                if runner.completion_rejection(&directive).is_none() {
                    Ok(AgentRoundOutcome::Complete(text))
                } else {
                    self.context.recent_messages.push(AgentMessage {
                        message_id: format!("sub_no_tools:{}", self.agent_id),
                        role: "user".into(),
                        content: "请至少调用一个工具并对目标进行实际验证后再给出结论。".into(),
                        format: "text".into(),
                    });
                    Ok(AgentRoundOutcome::Continue)
                }
            }
        }
    }
}

/// Execute a sub-agent's autonomous tool loop independently.
/// Reuses the parent's Run for evidence isolation, supports parallel tool execution.
async fn execute_sub_agent(
    agent_id: &str,
    session_id: &str,
    prompt: &str,
    target: Option<&str>,
    description: &str,
    child_budget: AgentChildBudget,
    repository: Arc<dyn ConversationRepository>,
    gateway: Arc<dyn ConversationModelGateway>,
    context_source: Arc<dyn ConversationContextSource>,
    knowledge_source: Arc<dyn ConversationKnowledgeSource>,
    tool_executor: Arc<dyn ConversationToolExecutor>,
    approval_gate: Arc<dyn ToolApprovalGate>,
    finding_recorder: Arc<dyn FindingRecorder>,
    skill_store: Arc<dyn SkillStore>,
    intent_detector: Arc<dyn ConversationIntentDetector>,
) -> anyhow::Result<String> {
    let objective = if let Some(t) = target {
        format!("对 {} 执行：{}", t, prompt)
    } else {
        prompt.to_string()
    };
    let request = AgentTurnRequest {
        session_id: session_id.to_string(),
        content: objective.clone(),
        message_type: ConversationMessageType::Chat,
        client_request_id: format!("sub-agent-{}", agent_id),
        attachments: vec![],
    };

    let mut context = context_source.build(&request).await?;
    let user_ledger_chars = std::env::var("PENTA_SUB_AGENT_USER_CONTEXT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(24_000);
    ContextManager::sub_agent_with_user_ledger_chars(user_ledger_chars)
        .initialize(&mut context, Some(format!("sub_context:{agent_id}")));
    let intent = intent_detector
        .detect(&request, context.run_id.is_some())
        .await?;

    // Reuse parent's existing Run
    let run_id = repository
        .ensure_execution_run(agent_id, session_id)
        .await?
        .unwrap_or_else(|| format!("sub-run-{}", agent_id));

    let budget = AgentLoopLimits {
        max_rounds: child_budget.max_rounds,
        max_duration_seconds: child_budget.max_duration_seconds,
        max_requests: child_budget.max_requests,
        max_tokens: child_budget.max_tokens,
        stall_limit: Some(child_budget.stall_limit),
    };
    let mut runner = AgentRunner::new(AgentRunConfig::sub_agent(budget));
    let mut context = context;
    context.run_id = Some(run_id.clone());

    // Inject sub-agent instructions
    context.recent_messages.push(AgentMessage {
        message_id: format!("sub_agent_start:{}", agent_id),
        role: "user".into(),
        content: format!(
            "## 子代理任务\n\n\
             你是一个专注于单一子任务的渗透测试子代理。\n\
             任务描述：{description}\n\
             任务目标：{objective}\n\n\
             **执行规则**：\n\
             - 专注于当前子任务，不要扩展开来扫描其他目标\n\
             - 直接调用工具执行，不要输出计划等待确认\n\
             - 任务完成 → `FINAL: <结论>`（必须引用工具输出作为证据）\n\
             - 遇到问题 → 直接 REPLAN 改策略继续，不要停下\n\
             - 确实无路可走 → `NO_PATH: <原因>`\n\
             - 需要关键信息 → `ASK_USER: <问题>`\n\
             - 子代理预算有限（{} 轮），请高效执行\n\n\
             **工具选择**：优先用专项工具（nuclei/sqlmap/dalfox/dir_fuzz/ffuf/js_analyze/browser MCP），\
             curl/http_request 仅用于特殊测试。先探测后攻击，工具失败就换工具不要重试。",
            runner.limits().max_rounds
        ),
        format: "text".into(),
    });

    let mut hooks = SubAgentRunHooks {
        agent_id,
        session_id,
        run_id: &run_id,
        context: &mut context,
        intent: &intent,
        repository: &repository,
        gateway: &gateway,
        knowledge_source: &knowledge_source,
        tool_executor: &tool_executor,
        approval_gate: &approval_gate,
        finding_recorder: &finding_recorder,
        skill_store: &skill_store,
    };
    let run_result = runner.run(&mut hooks).await?;
    let loop_checkpoint = run_result.checkpoint;
    let result = run_result.output.unwrap_or_else(|| {
        format!(
            "子代理 {}: {} 轮 {} 次请求后未生成结论（工具调用 {} 次，有效观察 {} 次）",
            description,
            runner.limits().max_rounds,
            loop_checkpoint.request_count,
            loop_checkpoint.tool_call_count,
            loop_checkpoint.effective_observations
        )
    });

    Ok(result)
}
