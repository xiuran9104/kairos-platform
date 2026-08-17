//! Runtime-owned sub-agent scheduling, quotas, cancellation and state events.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tokio::task::AbortHandle;

use crate::context_lifecycle::ContextCheckpoint;
use crate::ports::{AgentChildBudget, AgentSpawnRequest, RuntimeAgentStatus, RuntimeSubAgent};
use kairos_platform_pent_api::ConversationContext;

static NEXT_AGENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
pub struct AgentSchedulerPolicy {
    pub max_concurrent: usize,
    pub execution_timeout: Duration,
    pub collect_timeout: Duration,
    pub claim_renew_interval: Duration,
}

impl AgentSchedulerPolicy {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
            execution_timeout: Duration::from_secs(15 * 60),
            collect_timeout: Duration::from_secs(5 * 60),
            claim_renew_interval: Duration::from_secs(30),
        }
    }
}

/// Derive a bounded child budget from the parent's configured and consumed
/// budget. A child can never receive more remaining work than its parent.
pub fn inherit_child_budget(
    context: &ConversationContext,
    child_cap: AgentChildBudget,
) -> AgentChildBudget {
    let checkpoint = ContextCheckpoint::from_context(context).loop_state;
    let configured = &context.budget_snapshot;
    let value = |keys: &[&str], default: u64| {
        keys.iter()
            .find_map(|key| configured.get(key).and_then(serde_json::Value::as_u64))
            .unwrap_or(default)
    };
    let remaining = |total: u64, used: u64| total.saturating_sub(used).max(1);
    AgentChildBudget {
        max_rounds: child_cap.max_rounds.min(
            usize::try_from(remaining(
                value(&["max_tool_rounds", "maxToolRounds"], 50),
                checkpoint.next_round as u64,
            ))
            .unwrap_or(usize::MAX),
        ),
        max_duration_seconds: child_cap.max_duration_seconds.min(value(
            &[
                "max_duration_seconds",
                "maxDurationSeconds",
                "max_time_seconds",
            ],
            child_cap.max_duration_seconds,
        )),
        max_requests: child_cap.max_requests.min(
            usize::try_from(remaining(
                value(&["max_requests", "maxRequests"], 200),
                checkpoint.request_count as u64,
            ))
            .unwrap_or(usize::MAX),
        ),
        max_tokens: child_cap.max_tokens.min(remaining(
            value(&["max_tokens", "maxTokens"], 200_000),
            checkpoint.token_count,
        )),
        stall_limit: child_cap.stall_limit.max(1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSchedulerEventKind {
    Spawned,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl AgentSchedulerEventKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spawned => "agent.spawned",
            Self::Completed => "agent.completed",
            Self::Failed => "agent.failed",
            Self::Cancelled => "agent.cancelled",
            Self::TimedOut => "agent.timed_out",
        }
    }
}

#[async_trait::async_trait]
pub trait ScheduledAgentExecutor: Send + Sync {
    async fn execute(&self, agent_id: &str, request: AgentSpawnRequest) -> anyhow::Result<String>;
}

#[async_trait::async_trait]
pub trait AgentSchedulerEventSink: Send + Sync {
    async fn emit(
        &self,
        parent_run_id: &str,
        kind: AgentSchedulerEventKind,
        agent: &RuntimeSubAgent,
        request: Option<&AgentSpawnRequest>,
    ) -> anyhow::Result<()>;

    async fn load_unfinished(&self, _parent_run_id: &str) -> anyhow::Result<Vec<RecoveredAgent>> {
        Ok(Vec::new())
    }

    /// Renew the distributed execution claim for a running child. Returning
    /// `false` means ownership was lost and local execution must stop.
    async fn renew_claim(&self, _parent_run_id: &str, _agent_id: &str) -> anyhow::Result<bool> {
        Ok(true)
    }
}

#[derive(Debug, Clone)]
pub struct RecoveredAgent {
    pub state: RuntimeSubAgent,
    pub request: AgentSpawnRequest,
}

#[derive(Default)]
pub struct NoopAgentSchedulerEventSink;

#[async_trait::async_trait]
impl AgentSchedulerEventSink for NoopAgentSchedulerEventSink {
    async fn emit(
        &self,
        _parent_run_id: &str,
        _kind: AgentSchedulerEventKind,
        _agent: &RuntimeSubAgent,
        _request: Option<&AgentSpawnRequest>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

struct ScheduledAgentRecord {
    parent_run_id: String,
    state: RuntimeSubAgent,
}

#[derive(Default)]
struct SchedulerState {
    agents: HashMap<String, ScheduledAgentRecord>,
    tasks: HashMap<String, AbortHandle>,
}

pub struct AgentScheduler {
    policy: AgentSchedulerPolicy,
    executor: Arc<dyn ScheduledAgentExecutor>,
    events: Arc<dyn AgentSchedulerEventSink>,
    state: Arc<Mutex<SchedulerState>>,
    changed: Arc<Notify>,
}

impl AgentScheduler {
    pub fn new(
        policy: AgentSchedulerPolicy,
        executor: Arc<dyn ScheduledAgentExecutor>,
        events: Arc<dyn AgentSchedulerEventSink>,
    ) -> Arc<Self> {
        Arc::new(Self {
            policy,
            executor,
            events,
            state: Arc::new(Mutex::new(SchedulerState::default())),
            changed: Arc::new(Notify::new()),
        })
    }

    pub async fn active_count(&self) -> usize {
        self.state
            .lock()
            .await
            .agents
            .values()
            .filter(|agent| agent.state.status == RuntimeAgentStatus::Running)
            .count()
    }

    /// Atomically reserve quota, publish the spawn event, then launch work.
    pub async fn spawn(self: &Arc<Self>, request: AgentSpawnRequest) -> anyhow::Result<String> {
        let agent_id = next_agent_id();
        let agent = RuntimeSubAgent {
            agent_id: agent_id.clone(),
            description: request.description.clone(),
            status: RuntimeAgentStatus::Running,
            result: None,
            error: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            collected: false,
        };

        {
            let mut state = self.state.lock().await;
            let active = state
                .agents
                .values()
                .filter(|agent| agent.state.status == RuntimeAgentStatus::Running)
                .count();
            if active >= self.policy.max_concurrent {
                anyhow::bail!(
                    "已达到最大并发子代理数 ({})，请等待部分完成后重试",
                    self.policy.max_concurrent
                );
            }
            state.agents.insert(
                agent_id.clone(),
                ScheduledAgentRecord {
                    parent_run_id: request.parent_run_id.clone(),
                    state: agent.clone(),
                },
            );
        }

        if let Err(error) = self
            .events
            .emit(
                &request.parent_run_id,
                AgentSchedulerEventKind::Spawned,
                &agent,
                Some(&request),
            )
            .await
        {
            self.state.lock().await.agents.remove(&agent_id);
            return Err(error);
        }

        self.launch(agent_id.clone(), request).await;
        Ok(agent_id)
    }

    async fn launch(self: &Arc<Self>, agent_id: String, request: AgentSpawnRequest) {
        let scheduler = self.clone();
        let task_agent_id = agent_id.clone();
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if start_rx.await.is_err() {
                return;
            }
            let parent_run_id = request.parent_run_id.clone();
            let execution = tokio::time::timeout(
                scheduler.policy.execution_timeout,
                scheduler.executor.execute(&task_agent_id, request),
            );
            tokio::pin!(execution);
            let result = loop {
                tokio::select! {
                    output = &mut execution => break output,
                    _ = tokio::time::sleep(scheduler.policy.claim_renew_interval) => {
                        match scheduler.events.renew_claim(&parent_run_id, &task_agent_id).await {
                            Ok(true) => {}
                            Ok(false) => break Ok(Err(anyhow::anyhow!("子代理执行租约已丢失"))),
                            Err(error) => {
                                tracing::warn!(%error, agent_id = %task_agent_id, "failed to renew sub-agent claim");
                                break Ok(Err(anyhow::anyhow!("子代理执行租约续期失败: {error}")));
                            }
                        }
                    }
                }
            };
            scheduler.finish(&task_agent_id, result).await;
        });
        self.state
            .lock()
            .await
            .tasks
            .insert(agent_id.clone(), task.abort_handle());
        let _ = start_tx.send(());
    }

    pub async fn recover(self: &Arc<Self>, parent_run_id: &str) -> anyhow::Result<usize> {
        let recovered = self.events.load_unfinished(parent_run_id).await?;
        let mut restored = 0;
        for item in recovered {
            let agent_id = item.state.agent_id.clone();
            {
                let mut state = self.state.lock().await;
                let active = state
                    .agents
                    .values()
                    .filter(|agent| agent.state.status == RuntimeAgentStatus::Running)
                    .count();
                if state.agents.contains_key(&agent_id) || active >= self.policy.max_concurrent {
                    continue;
                }
                state.agents.insert(
                    agent_id.clone(),
                    ScheduledAgentRecord {
                        parent_run_id: parent_run_id.to_string(),
                        state: item.state,
                    },
                );
            }
            self.launch(agent_id, item.request).await;
            restored += 1;
        }
        Ok(restored)
    }

    async fn finish(
        &self,
        agent_id: &str,
        result: Result<anyhow::Result<String>, tokio::time::error::Elapsed>,
    ) {
        let (parent_run_id, kind, snapshot) = {
            let mut state = self.state.lock().await;
            state.tasks.remove(agent_id);
            let Some(record) = state.agents.get_mut(agent_id) else {
                return;
            };
            // A parent cancellation wins over a concurrently finishing task.
            if record.state.status != RuntimeAgentStatus::Running {
                return;
            }
            let kind = match result {
                Ok(Ok(content)) => {
                    record.state.status = RuntimeAgentStatus::Completed;
                    record.state.result = Some(content);
                    AgentSchedulerEventKind::Completed
                }
                Ok(Err(error)) => {
                    record.state.status = RuntimeAgentStatus::Failed;
                    record.state.error = Some(error.to_string());
                    AgentSchedulerEventKind::Failed
                }
                Err(_) => {
                    record.state.status = RuntimeAgentStatus::Failed;
                    record.state.error = Some("子代理执行超时".into());
                    AgentSchedulerEventKind::TimedOut
                }
            };
            record.state.completed_at = Some(chrono::Utc::now().to_rfc3339());
            (record.parent_run_id.clone(), kind, record.state.clone())
        };
        self.changed.notify_waiters();
        if let Err(error) = self
            .events
            .emit(&parent_run_id, kind, &snapshot, None)
            .await
        {
            tracing::warn!(%error, agent_id, event = kind.as_str(), "failed to persist agent scheduler event");
        }
    }

    pub async fn status(&self, agent_id: &str) -> Option<RuntimeSubAgent> {
        self.state
            .lock()
            .await
            .agents
            .get(agent_id)
            .map(|record| record.state.clone())
    }

    pub async fn list(&self) -> Vec<RuntimeSubAgent> {
        let mut agents = self
            .state
            .lock()
            .await
            .agents
            .values()
            .map(|record| record.state.clone())
            .collect::<Vec<_>>();
        agents.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        agents
    }

    pub async fn collect(&self, agent_id: &str) -> Option<String> {
        let deadline = tokio::time::Instant::now() + self.policy.collect_timeout;
        loop {
            let notified = self.changed.notified();
            match self.status(agent_id).await {
                Some(agent) if agent.status == RuntimeAgentStatus::Completed => {
                    self.mark_collected(agent_id).await;
                    return agent.result;
                }
                Some(agent) if agent.status == RuntimeAgentStatus::Failed => {
                    self.mark_collected(agent_id).await;
                    return agent.error.map(|error| format!("子代理执行失败: {error}"));
                }
                Some(agent) if agent.status == RuntimeAgentStatus::Cancelled => {
                    self.mark_collected(agent_id).await;
                    return agent.error.or_else(|| Some("子代理已取消".into()));
                }
                None => return Some("子代理不存在".into()),
                Some(_) => {}
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Some("等待子代理结果超时".into());
            }
        }
    }

    async fn mark_collected(&self, agent_id: &str) {
        if let Some(record) = self.state.lock().await.agents.get_mut(agent_id) {
            record.state.collected = true;
        }
    }

    /// Cancel every running child and abort its actual Tokio task.
    pub async fn abort_all(&self) {
        let cancelled = {
            let mut state = self.state.lock().await;
            let handles = state
                .tasks
                .drain()
                .map(|(_, handle)| handle)
                .collect::<Vec<_>>();
            for handle in handles {
                handle.abort();
            }
            let mut cancelled = Vec::new();
            for record in state.agents.values_mut() {
                if record.state.status == RuntimeAgentStatus::Running {
                    record.state.status = RuntimeAgentStatus::Cancelled;
                    record.state.error = Some("父任务已结束，子代理被取消".into());
                    record.state.completed_at = Some(chrono::Utc::now().to_rfc3339());
                    cancelled.push((record.parent_run_id.clone(), record.state.clone()));
                }
            }
            cancelled
        };
        self.changed.notify_waiters();
        for (parent_run_id, agent) in cancelled {
            if let Err(error) = self
                .events
                .emit(
                    &parent_run_id,
                    AgentSchedulerEventKind::Cancelled,
                    &agent,
                    None,
                )
                .await
            {
                tracing::warn!(%error, agent_id = %agent.agent_id, "failed to persist agent cancellation");
            }
        }
    }
}

fn next_agent_id() -> String {
    let sequence = NEXT_AGENT_ID.fetch_add(1, Ordering::Relaxed);
    let millis = chrono::Utc::now().timestamp_millis();
    format!("sub-{millis:x}-{sequence:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ImmediateExecutor;

    #[async_trait::async_trait]
    impl ScheduledAgentExecutor for ImmediateExecutor {
        async fn execute(
            &self,
            agent_id: &str,
            _request: AgentSpawnRequest,
        ) -> anyhow::Result<String> {
            Ok(format!("done:{agent_id}"))
        }
    }

    struct PendingExecutor;

    #[async_trait::async_trait]
    impl ScheduledAgentExecutor for PendingExecutor {
        async fn execute(
            &self,
            _agent_id: &str,
            _request: AgentSpawnRequest,
        ) -> anyhow::Result<String> {
            std::future::pending().await
        }
    }

    struct RecoverySink {
        recovered: Vec<RecoveredAgent>,
    }

    struct LeaseLostSink;

    #[async_trait::async_trait]
    impl AgentSchedulerEventSink for LeaseLostSink {
        async fn emit(
            &self,
            _parent_run_id: &str,
            _kind: AgentSchedulerEventKind,
            _agent: &RuntimeSubAgent,
            _request: Option<&AgentSpawnRequest>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn renew_claim(&self, _parent_run_id: &str, _agent_id: &str) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    #[async_trait::async_trait]
    impl AgentSchedulerEventSink for RecoverySink {
        async fn emit(
            &self,
            _parent_run_id: &str,
            _kind: AgentSchedulerEventKind,
            _agent: &RuntimeSubAgent,
            _request: Option<&AgentSpawnRequest>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn load_unfinished(
            &self,
            _parent_run_id: &str,
        ) -> anyhow::Result<Vec<RecoveredAgent>> {
            Ok(self.recovered.clone())
        }
    }

    fn request(index: usize) -> AgentSpawnRequest {
        AgentSpawnRequest {
            description: format!("task {index}"),
            prompt: "test".into(),
            session_id: "session".into(),
            parent_run_id: "parent".into(),
            target: None,
            child_budget: AgentChildBudget {
                max_rounds: 12,
                max_duration_seconds: 60,
                max_requests: 80,
                max_tokens: 10_000,
                stall_limit: 4,
            },
        }
    }

    #[tokio::test]
    async fn completes_and_collects_normalized_result() {
        let scheduler = AgentScheduler::new(
            AgentSchedulerPolicy::new(1),
            Arc::new(ImmediateExecutor),
            Arc::new(NoopAgentSchedulerEventSink),
        );
        let id = scheduler.spawn(request(1)).await.unwrap();
        let result = scheduler.collect(&id).await.unwrap();
        assert_eq!(result, format!("done:{id}"));
        assert_eq!(
            scheduler.status(&id).await.unwrap().status,
            RuntimeAgentStatus::Completed
        );
        assert!(scheduler.status(&id).await.unwrap().collected);
    }

    #[tokio::test]
    async fn quota_admission_is_atomic_and_abort_propagates() {
        let scheduler = AgentScheduler::new(
            AgentSchedulerPolicy::new(1),
            Arc::new(PendingExecutor),
            Arc::new(NoopAgentSchedulerEventSink),
        );
        let id = scheduler.spawn(request(1)).await.unwrap();
        assert!(scheduler.spawn(request(2)).await.is_err());
        scheduler.abort_all().await;
        assert_eq!(scheduler.active_count().await, 0);
        assert_eq!(
            scheduler.status(&id).await.unwrap().status,
            RuntimeAgentStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn execution_timeout_updates_terminal_state() {
        let scheduler = AgentScheduler::new(
            AgentSchedulerPolicy {
                max_concurrent: 1,
                execution_timeout: Duration::from_millis(5),
                collect_timeout: Duration::from_secs(1),
                claim_renew_interval: Duration::from_secs(1),
            },
            Arc::new(PendingExecutor),
            Arc::new(NoopAgentSchedulerEventSink),
        );
        let id = scheduler.spawn(request(1)).await.unwrap();
        let result = scheduler.collect(&id).await.unwrap();
        assert!(result.contains("超时"));
    }

    #[tokio::test]
    async fn recovery_relaunches_unfinished_agent_with_original_id() {
        let restored_id = "sub-restored".to_string();
        let scheduler = AgentScheduler::new(
            AgentSchedulerPolicy::new(1),
            Arc::new(ImmediateExecutor),
            Arc::new(RecoverySink {
                recovered: vec![RecoveredAgent {
                    state: RuntimeSubAgent {
                        agent_id: restored_id.clone(),
                        description: "restored task".into(),
                        status: RuntimeAgentStatus::Running,
                        result: None,
                        error: None,
                        started_at: chrono::Utc::now().to_rfc3339(),
                        completed_at: None,
                        collected: false,
                    },
                    request: request(1),
                }],
            }),
        );

        assert_eq!(scheduler.recover("parent").await.unwrap(), 1);
        assert_eq!(
            scheduler.collect(&restored_id).await.as_deref(),
            Some("done:sub-restored")
        );
        assert_eq!(
            scheduler.status(&restored_id).await.unwrap().status,
            RuntimeAgentStatus::Completed
        );
    }

    #[tokio::test]
    async fn lost_distributed_claim_stops_local_execution() {
        let scheduler = AgentScheduler::new(
            AgentSchedulerPolicy {
                max_concurrent: 1,
                execution_timeout: Duration::from_secs(1),
                collect_timeout: Duration::from_secs(1),
                claim_renew_interval: Duration::from_millis(1),
            },
            Arc::new(PendingExecutor),
            Arc::new(LeaseLostSink),
        );
        let id = scheduler.spawn(request(1)).await.unwrap();
        let result = scheduler.collect(&id).await.unwrap();
        assert!(result.contains("租约已丢失"));
        assert_eq!(
            scheduler.status(&id).await.unwrap().status,
            RuntimeAgentStatus::Failed
        );
    }

    #[test]
    fn child_budget_is_capped_by_parent_remaining_budget() {
        let context = kairos_platform_pent_api::ConversationContext {
            context_version: 1,
            session_id: "session".into(),
            run_id: Some("run".into()),
            system_instruction: String::new(),
            objective_snapshot: String::new(),
            scope_snapshot: serde_json::json!({}),
            policy_snapshot: serde_json::json!({
                "_checkpoint": {"round": 8, "request_count": 19, "token_count": 900}
            }),
            budget_snapshot: serde_json::json!({
                "max_tool_rounds": 10, "max_requests": 20, "max_tokens": 1000
            }),
            run_summary: String::new(),
            recent_messages: Vec::new(),
            evidence_refs: Vec::new(),
            memory_refs: Vec::new(),
            tool_ids: Vec::new(),
            estimated_tokens: 0,
        };
        let budget = inherit_child_budget(&context, request(1).child_budget);
        assert_eq!(budget.max_rounds, 2);
        assert_eq!(budget.max_requests, 1);
        assert_eq!(budget.max_tokens, 100);
    }
}
