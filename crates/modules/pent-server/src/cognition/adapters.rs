//! Real cognition-port implementations (v1, deterministic per the H-PER-M
//! invariants: executors and meta-monitoring never use an LLM).

use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::cognition::*;
use kairos_platform_pent_api::{ToolExecutionResult, ToolExecutionStatus};
use serde_json::{Value, json};
use sha2::Digest;

use super::ports::{
    ActionReflector, CognitionEventSink, CognitiveExecutor, DynamicReplanner, MetaMetricsSource,
    PreReflector, StepPlanner,
};

// ── Step planner: pick executable actions from the TCG ──────────────────

pub struct DeterministicStepPlanner;

/// The next action to execute: Pending/Ready, attempts left, parent stage
/// Running or Completed, lowest priority number first (1 = run first).
/// `None` means the driver should expand the next stage or finish.
pub fn select_next_action(graph: &TaskCoordinationGraph) -> Option<&PlanNode> {
    graph
        .nodes
        .iter()
        .filter(|node| node.node_type == PlanNodeType::Action)
        .filter(|node| {
            matches!(node.status, PlanNodeStatus::Pending | PlanNodeStatus::Ready)
        })
        .filter(|node| node.attempt < node.max_attempts)
        .filter(|node| parent_allows(graph, node))
        .min_by_key(|node| node.priority)
}

/// All executable actions across all Running stages, sorted by priority.
/// Used for parallel fan-out: up to `max_concurrency` actions can be
/// dispatched concurrently.
pub fn select_all_executable_actions(graph: &TaskCoordinationGraph) -> Vec<&PlanNode> {
    let mut actions: Vec<&PlanNode> = graph
        .nodes
        .iter()
        .filter(|node| node.node_type == PlanNodeType::Action)
        .filter(|node| {
            matches!(node.status, PlanNodeStatus::Pending | PlanNodeStatus::Ready)
        })
        .filter(|node| node.attempt < node.max_attempts)
        .filter(|node| parent_allows(graph, node))
        .collect();
    actions.sort_by_key(|node| node.priority);
    actions
}

fn parent_allows(graph: &TaskCoordinationGraph, node: &PlanNode) -> bool {
    match &node.parent_id {
        None => true,
        Some(parent_id) => graph
            .nodes
            .iter()
            .find(|parent| &parent.node_id == parent_id)
            .is_some_and(|parent| {
                matches!(
                    parent.status,
                    PlanNodeStatus::Running | PlanNodeStatus::Completed
                )
            }),
    }
}

/// A Pending stage whose Requires dependencies are all Completed.
pub fn select_expandable_stage(graph: &TaskCoordinationGraph) -> Option<&PlanNode> {
    graph
        .nodes
        .iter()
        .filter(|node| node.node_type == PlanNodeType::Stage)
        .filter(|node| node.status == PlanNodeStatus::Pending)
        .filter(|stage| stage_dependencies_met(graph, stage))
        .min_by_key(|node| node.priority)
}

/// All Pending stages whose Requires dependencies are all Completed,
/// sorted by priority. These can be expanded and executed in parallel
/// (subject to `PlanBudget.max_concurrency`).
pub fn select_all_expandable_stages(graph: &TaskCoordinationGraph) -> Vec<&PlanNode> {
    let mut stages: Vec<&PlanNode> = graph
        .nodes
        .iter()
        .filter(|node| node.node_type == PlanNodeType::Stage)
        .filter(|node| node.status == PlanNodeStatus::Pending)
        .filter(|stage| stage_dependencies_met(graph, stage))
        .collect();
    stages.sort_by_key(|node| node.priority);
    stages
}

fn stage_dependencies_met(graph: &TaskCoordinationGraph, stage: &PlanNode) -> bool {
    graph
        .edges
        .iter()
        .filter(|edge| edge.to == stage.node_id && edge.edge_type == PlanEdgeType::Requires)
        .all(|edge| {
            graph
                .nodes
                .iter()
                .find(|node| node.node_id == edge.from)
                .is_some_and(|dep| dep.status == PlanNodeStatus::Completed)
        })
}

fn risk_for_capability(capability: &str) -> String {
    match capability {
        "sqli_validate" | "xss_validate" | "csrf_validate" | "command_injection_validate"
        | "file_inclusion_validate" | "file_upload_validate" => "medium".into(),
        _ => "low".into(),
    }
}

#[async_trait]
impl StepPlanner for DeterministicStepPlanner {
    async fn propose(&self, graph: &TaskCoordinationGraph) -> anyhow::Result<ActionProposal> {
        let node = select_next_action(graph)
            .ok_or_else(|| anyhow::anyhow!("NO_EXECUTABLE_ACTION"))?;
        Ok(ActionProposal {
            action_id: uuid::Uuid::new_v4().to_string(),
            plan_id: graph.plan_id.clone(),
            plan_version: graph.version,
            node_id: node.node_id.clone(),
            capability: node.capability.clone().unwrap_or_default(),
            target: node.target.clone().unwrap_or_default(),
            argument_template: node.success_criteria.clone(),
            expected_outputs: vec![],
            risk_level: risk_for_capability(node.capability.as_deref().unwrap_or("")),
        })
    }
}

// ── PreFlect: deterministic scope / duplicate / budget gate ──────────────

pub struct ScopePreflector {
    scope_hosts: Vec<String>,
}

impl ScopePreflector {
    pub fn new(scope_snapshot: &Value) -> Self {
        let targets = scope_snapshot
            .get("targets")
            .and_then(Value::as_array)
            .map(|targets| {
                targets
                    .iter()
                    .filter_map(|target| target.as_str().map(host_of))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            scope_hosts: targets,
        }
    }
}

/// Extract the comparable host part from a target (URL or bare host[:port]).
pub fn host_of(target: &str) -> String {
    let without_scheme = target
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(target);
    without_scheme
        .split(['/', ':', '?'])
        .next()
        .unwrap_or(without_scheme)
        .trim()
        .to_ascii_lowercase()
}

pub fn target_in_scope(target: &str, scope_hosts: &[String]) -> bool {
    let host = host_of(target);
    !host.is_empty() && scope_hosts.contains(&host)
}

#[async_trait]
impl PreReflector for ScopePreflector {
    async fn evaluate(
        &self,
        graph: &TaskCoordinationGraph,
        proposal: &ActionProposal,
    ) -> anyhow::Result<PreFlection> {
        let scope_valid = target_in_scope(&proposal.target, &self.scope_hosts);
        let duplicate = graph.nodes.iter().any(|node| {
            node.node_type == PlanNodeType::Action
                && node.status == PlanNodeStatus::Completed
                && node.capability.as_deref() == Some(proposal.capability.as_str())
                && node.target.as_deref() == Some(proposal.target.as_str())
        });
        Ok(kairos_platform_pent_core::cognition::deterministic_preflect(
            proposal,
            duplicate,
            true,
            scope_valid,
        ))
    }
}

// ── Executor: real sandboxed tool execution + evidence persistence ───────

pub struct ProcessExecutor {
    pool: PgPool,
    run_id: String,
    backend: crate::infrastructure::process::ExecutionBackend,
}

impl ProcessExecutor {
    pub fn new(pool: PgPool, run_id: &str) -> Self {
        Self {
            pool,
            run_id: run_id.to_string(),
            backend: crate::infrastructure::process::ExecutionBackend::from_env(),
        }
    }

    /// Explicit backend override (tests, operator-forced execution modes).
    pub fn with_backend(
        pool: PgPool,
        run_id: &str,
        backend: crate::infrastructure::process::ExecutionBackend,
    ) -> Self {
        Self {
            pool,
            run_id: run_id.to_string(),
            backend,
        }
    }

    fn command_for(proposal: &ActionProposal) -> Result<(String, Vec<String>), String> {
        match proposal.capability.as_str() {
            "tcp_scan" | "scanner_nmap" => {
                Ok(("tcp_scan".into(), vec!["scan".into(), proposal.target.clone()]))
            }
            "http_probe" | "scanner_nuclei" => Ok((
                "http_probe".into(),
                vec!["probe".into(), proposal.target.clone()],
            )),
            "dns_lookup" => Ok(("dns_lookup".into(), vec![proposal.target.clone()])),
            other => Err(format!("认知执行器暂不支持能力 '{other}'")),
        }
    }
}

#[async_trait]
impl CognitiveExecutor for ProcessExecutor {
    async fn execute(&self, proposal: &ActionProposal) -> anyhow::Result<ToolExecutionResult> {
        let (tool_name, command) = Self::command_for(proposal)
            .map_err(|error| anyhow::anyhow!(error))?;
        let limits = crate::infrastructure::process::ContainerLimits {
            timeout_seconds: 120,
            network_access: crate::infrastructure::process::NetworkPolicy::TargetOnly(
                proposal.target.clone(),
            ),
            backend: self.backend,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let output = crate::infrastructure::process::execute_tool(&tool_name, &command, &limits)
            .await
            .map_err(|error| anyhow::anyhow!("工具执行失败: {error}"))?;
        let result = kairos_platform_pent_core::tools::adapters::normalize_security_tool_output(
            &tool_name,
            &proposal.target,
            output.exit_code,
            &output.stdout,
            &output.stderr,
            output.duration_ms,
        );

        // Persist tool call + raw evidence so the fact layer stays complete.
        let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let backend_name = match limits.backend {
            crate::infrastructure::process::ExecutionBackend::Docker => "docker",
            crate::infrastructure::process::ExecutionBackend::Host => "host",
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pent_tool_calls(
                 id,run_id,tool_name,params_summary,permission_decision,
                 started_at,completed_at,duration_ms,exit_code,executor_backend,execution_audit
             ) VALUES($1,$2,$3,$4,'allowed',now(),now(),$5,$6,$7,$8)",
        )
        .bind(&proposal.action_id)
        .bind(&self.run_id)
        .bind(&tool_name)
        .bind(json!({"target": proposal.target, "capability": proposal.capability}))
        .bind(duration_ms)
        .bind(output.exit_code)
        .bind(backend_name)
        .bind(json!({
            "backend": backend_name,
            "plan_id": proposal.plan_id,
            "plan_version": proposal.plan_version,
            "node_id": proposal.node_id,
        }))
        .execute(&mut *tx)
        .await?;
        let evidence_id = uuid::Uuid::new_v4().to_string();
        let source = serde_json::to_string(&result)?;
        let hash = format!(
            "sha256:{:x}",
            sha2::Sha256::digest(source.as_bytes())
        );
        sqlx::query(
            "INSERT INTO pent_evidence(id,run_id,tool_call_id,source,content_hash,content_type)
             VALUES($1,$2,$3,$4,$5,'application/json')",
        )
        .bind(&evidence_id)
        .bind(&self.run_id)
        .bind(&proposal.action_id)
        .bind(&source)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        let mut result = result;
        result.raw_output_ref = Some(evidence_id);
        Ok(result)
    }
}

// ── Reflector: deterministic usefulness / information-gain scoring ───────

pub struct HeuristicReflector;

#[async_trait]
impl ActionReflector for HeuristicReflector {
    async fn reflect(
        &self,
        _graph: &TaskCoordinationGraph,
        proposal: &ActionProposal,
        result: &ToolExecutionResult,
    ) -> anyhow::Result<ActionReflection> {
        let useful = matches!(result.status, ToolExecutionStatus::Succeeded)
            && (!result.observations.is_empty() || !result.findings.is_empty());
        let (information_gain, reason_codes, lessons) = match (&result.status, useful) {
            (ToolExecutionStatus::Succeeded, true) => (0.6, vec![], vec![]),
            (ToolExecutionStatus::Succeeded, false) => (
                0.2,
                vec!["NO_NEW_OBSERVATION".into()],
                vec![],
            ),
            (ToolExecutionStatus::Partial, _) => (0.3, vec!["PARTIAL_RESULT".into()], vec![]),
            _ => (
                0.0,
                vec!["TOOL_FAILED".into()],
                vec![format!(
                    "能力 {} 在目标 {} 上失败：{}",
                    proposal.capability, proposal.target, result.summary
                )],
            ),
        };
        Ok(ActionReflection {
            reflection_id: uuid::Uuid::new_v4().to_string(),
            action_id: proposal.action_id.clone(),
            useful,
            information_gain,
            tool_selection_quality: 0.7,
            result_quality: if useful { 0.8 } else { 0.3 },
            retry: !useful,
            reason_codes,
            lessons,
        })
    }
}

// ── Replanner: real PlanDelta operations for retry / tool fallback ───────

pub struct RuleReplanner;

/// Fallback capability when a tool is exhausted. `None` → mark failed.
fn alternative_capability(capability: &str) -> Option<&'static str> {
    match capability {
        "tcp_scan" | "scanner_nmap" => Some("http_probe"),
        "http_probe" | "scanner_nuclei" => Some("dns_lookup"),
        _ => None,
    }
}

#[async_trait]
impl DynamicReplanner for RuleReplanner {
    async fn replan(
        &self,
        graph: &TaskCoordinationGraph,
        proposal: &ActionProposal,
        _result: &ToolExecutionResult,
        reflection: &ActionReflection,
        _meta: &MetaCognitiveSnapshot,
    ) -> anyhow::Result<Option<PlanDelta>> {
        if !reflection.retry {
            return Ok(None);
        }
        let Some(node) = graph
            .nodes
            .iter()
            .find(|node| node.node_id == proposal.node_id)
        else {
            return Ok(None);
        };
        if node.attempt + 1 < node.max_attempts {
            // L1: retry the same action with a bumped attempt counter.
            return Ok(Some(PlanDelta {
                delta_id: uuid::Uuid::new_v4().to_string(),
                plan_id: graph.plan_id.clone(),
                base_version: graph.version,
                replan_level: ReplanLevel::Parameter,
                reason_codes: reflection.reason_codes.clone(),
                operations: vec![PlanDeltaOperation::UpdateNode {
                    node_id: node.node_id.clone(),
                    patch: json!({"attempt": node.attempt + 1, "status": "pending"}),
                }],
                proposed_by: "rule-replanner".into(),
                created_at: Utc::now(),
            }));
        }
        // L2: tool fallback — add an alternative action under the same stage.
        let mut operations = vec![PlanDeltaOperation::UpdateNode {
            node_id: node.node_id.clone(),
            patch: json!({"status": "failed"}),
        }];
        let mut level = ReplanLevel::Stage;
        if let Some(alternative) = alternative_capability(&proposal.capability) {
            let mut alt_node = node.clone();
            alt_node.node_id = format!("{}-alt", node.node_id);
            alt_node.capability = Some(alternative.into());
            alt_node.objective = format!("执行{alternative}（{} 的替代）", proposal.capability);
            alt_node.status = PlanNodeStatus::Pending;
            alt_node.attempt = 0;
            operations.push(PlanDeltaOperation::AddNode { node: alt_node });
            level = ReplanLevel::Tool;
        }
        Ok(Some(PlanDelta {
            delta_id: uuid::Uuid::new_v4().to_string(),
            plan_id: graph.plan_id.clone(),
            base_version: graph.version,
            replan_level: level,
            reason_codes: reflection.reason_codes.clone(),
            operations,
            proposed_by: "rule-replanner".into(),
            created_at: Utc::now(),
        }))
    }
}

// ── Meta metrics: real signals from the database ─────────────────────────

pub struct DbMetaMetrics {
    pool: PgPool,
}

impl DbMetaMetrics {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MetaMetricsSource for DbMetaMetrics {
    async fn collect(
        &self,
        run_id: &str,
        _graph: &TaskCoordinationGraph,
    ) -> anyhow::Result<MetaMetrics> {
        let recent: Vec<(i32, Option<String>)> = sqlx::query_as(
            "SELECT COALESCE(exit_code,-1), execution_audit->>'semantic_fingerprint'
             FROM pent_tool_calls WHERE run_id=$1
             ORDER BY started_at DESC NULLS LAST, created_at DESC LIMIT 10",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();
        let failure_streak = recent
            .iter()
            .take_while(|(exit_code, _)| *exit_code != 0)
            .count() as u32;
        let successes = recent.iter().filter(|(exit_code, _)| *exit_code == 0).count();
        let observation_confidence = if recent.is_empty() {
            0.8
        } else {
            successes as f32 / recent.len() as f32
        };
        let fingerprints: Vec<&str> = recent
            .iter()
            .filter_map(|(_, fp)| fp.as_deref())
            .collect();
        let unique: std::collections::HashSet<&&str> = fingerprints.iter().collect();
        let loop_similarity = if fingerprints.len() < 2 {
            0.0
        } else {
            1.0 - (unique.len() as f32 / fingerprints.len() as f32)
        };
        let total_calls: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_tool_calls WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
        let evidence_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_evidence WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
        let blocked_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pent_run_events
             WHERE run_id=$1 AND event_type LIKE '%scope_block%'",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(MetaMetrics {
            plan_confidence: 0.8,
            observation_confidence,
            evidence_coverage: (evidence_count as f32 / 10.0).min(1.0),
            recent_information_gain: 0.3,
            loop_similarity,
            failure_streak,
            budget_pressure: (total_calls as f32 / 50.0).min(1.0),
            scope_risk: if blocked_events > 0 { 0.6 } else { 0.0 },
            model_disagreement: 0.0,
        })
    }
}

// ── Event sink: reuse the run-event journal ──────────────────────────────

pub struct RunEventSink {
    pool: PgPool,
    run_id: String,
}

impl RunEventSink {
    pub fn new(pool: PgPool, run_id: &str) -> Self {
        Self {
            pool,
            run_id: run_id.to_string(),
        }
    }
}

#[async_trait]
impl CognitionEventSink for RunEventSink {
    async fn emit(&self, event_type: &str, payload: Value) -> anyhow::Result<()> {
        crate::append_run_event(&self.pool, &self.run_id, event_type, payload)
            .await
            .map_err(anyhow::Error::msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_platform_pent_api::{ToolExecutionMetrics, ToolObservation};

    fn node(
        id: &str,
        node_type: PlanNodeType,
        status: PlanNodeStatus,
        priority: i32,
        parent: Option<&str>,
        capability: Option<&str>,
        target: Option<&str>,
        attempt: u32,
        max_attempts: u32,
    ) -> PlanNode {
        PlanNode {
            node_id: id.into(),
            node_type,
            parent_id: parent.map(str::to_owned),
            namespace: "pentest".into(),
            objective: id.into(),
            capability: capability.map(str::to_owned),
            target: target.map(str::to_owned),
            status,
            priority,
            success_criteria: json!({}),
            budget: PlanBudget {
                max_actions: 10,
                max_tool_calls: 10,
                max_duration_seconds: 600,
                max_tokens: 1000,
                max_cost: 1.0,
                max_concurrency: 1,
            },
            attempt,
            max_attempts,
            observation_refs: vec![],
            valid_until: None,
        }
    }

    fn graph(nodes: Vec<PlanNode>, edges: Vec<PlanEdge>) -> TaskCoordinationGraph {
        TaskCoordinationGraph {
            plan_id: "plan".into(),
            run_id: "run".into(),
            version: 1,
            objective: "test".into(),
            nodes,
            edges,
            global_budget: PlanBudget {
                max_actions: 10,
                max_tool_calls: 10,
                max_duration_seconds: 600,
                max_tokens: 1000,
                max_cost: 1.0,
                max_concurrency: 1,
            },
            memory_refs: vec![],
            created_at: Utc::now(),
        }
    }

    fn proposal(capability: &str, target: &str, node_id: &str) -> ActionProposal {
        ActionProposal {
            action_id: "a-1".into(),
            plan_id: "plan".into(),
            plan_version: 1,
            node_id: node_id.into(),
            capability: capability.into(),
            target: target.into(),
            argument_template: json!({}),
            expected_outputs: vec![],
            risk_level: "low".into(),
        }
    }

    fn tool_result(status: ToolExecutionStatus, observations: usize) -> ToolExecutionResult {
        ToolExecutionResult {
            schema_version: 1,
            tool_name: "tcp_scan".into(),
            target: "127.0.0.1".into(),
            status,
            summary: "done".into(),
            observations: (0..observations)
                .map(|index| ToolObservation {
                    category: "tool_output".into(),
                    key: format!("obs-{index}"),
                    value: json!(index),
                    confidence: 0.9,
                })
                .collect(),
            findings: vec![],
            artifacts: vec![],
            raw_output_ref: None,
            metrics: ToolExecutionMetrics {
                exit_code: 0,
                duration_ms: 10,
            },
            warnings: vec![],
            occurred_at: Utc::now(),
        }
    }

    #[test]
    fn next_action_prefers_lowest_priority_number_with_parent_running() {
        let g = graph(
            vec![
                node("s1", PlanNodeType::Stage, PlanNodeStatus::Running, 1, None, None, None, 0, 1),
                node("s2", PlanNodeType::Stage, PlanNodeStatus::Pending, 2, None, None, None, 0, 1),
                node("a1", PlanNodeType::Action, PlanNodeStatus::Pending, 1, Some("s1"), Some("tcp_scan"), Some("127.0.0.1"), 0, 2),
                node("a2", PlanNodeType::Action, PlanNodeStatus::Pending, 5, Some("s1"), Some("http_probe"), Some("127.0.0.1"), 0, 2),
                node("a3", PlanNodeType::Action, PlanNodeStatus::Pending, 0, Some("s2"), Some("dns_lookup"), Some("127.0.0.1"), 0, 2),
                node("a4", PlanNodeType::Action, PlanNodeStatus::Pending, 0, Some("s1"), Some("x"), Some("127.0.0.1"), 2, 2),
            ],
            vec![],
        );
        let selected = select_next_action(&g).expect("an action is selectable");
        assert_eq!(selected.node_id, "a1", "a3 被父阶段阻塞、a4 尝试次数耗尽，优先级 1 最先执行");
    }

    #[test]
    fn expandable_stage_requires_dependencies_completed() {
        let g = graph(
            vec![
                node("s1", PlanNodeType::Stage, PlanNodeStatus::Completed, 1, None, None, None, 0, 1),
                node("s2", PlanNodeType::Stage, PlanNodeStatus::Pending, 2, None, None, None, 0, 1),
            ],
            vec![PlanEdge {
                from: "s1".into(),
                to: "s2".into(),
                edge_type: PlanEdgeType::Requires,
                condition: Value::Null,
            }],
        );
        assert_eq!(
            select_expandable_stage(&g).map(|stage| stage.node_id.as_str()),
            Some("s2")
        );
        let g2 = graph(
            vec![
                node("s1", PlanNodeType::Stage, PlanNodeStatus::Running, 1, None, None, None, 0, 1),
                node("s2", PlanNodeType::Stage, PlanNodeStatus::Pending, 2, None, None, None, 0, 1),
            ],
            vec![PlanEdge {
                from: "s1".into(),
                to: "s2".into(),
                edge_type: PlanEdgeType::Requires,
                condition: Value::Null,
            }],
        );
        assert!(select_expandable_stage(&g2).is_none());
    }

    #[test]
    fn scope_matching_normalizes_urls_and_ports() {
        assert_eq!(host_of("http://Example.com:8080/a?b=1"), "example.com");
        assert_eq!(host_of("127.0.0.1"), "127.0.0.1");
        assert!(target_in_scope("http://127.0.0.1:9000/x", &["127.0.0.1".into()]));
        assert!(!target_in_scope("http://evil.example", &["127.0.0.1".into()]));
        assert!(!target_in_scope("", &["127.0.0.1".into()]));
    }

    #[tokio::test]
    async fn preflector_blocks_out_of_scope_and_marks_duplicates() {
        let preflector = ScopePreflector {
            scope_hosts: vec!["127.0.0.1".into()],
        };
        let g = graph(vec![], vec![]);
        let blocked = preflector
            .evaluate(&g, &proposal("tcp_scan", "10.0.0.9", "a1"))
            .await
            .unwrap();
        assert_eq!(blocked.directive, PreFlectDirective::Block);
        assert!(blocked.risk_codes.contains(&"SCOPE_DENIED".to_string()));

        let g_dup = graph(
            vec![node(
                "a-old",
                PlanNodeType::Action,
                PlanNodeStatus::Completed,
                1,
                None,
                Some("tcp_scan"),
                Some("127.0.0.1"),
                1,
                1,
            )],
            vec![],
        );
        let duplicate = preflector
            .evaluate(&g_dup, &proposal("tcp_scan", "127.0.0.1", "a1"))
            .await
            .unwrap();
        assert_eq!(duplicate.directive, PreFlectDirective::Replace);

        let fresh = preflector
            .evaluate(&g, &proposal("tcp_scan", "127.0.0.1", "a1"))
            .await
            .unwrap();
        assert_eq!(fresh.directive, PreFlectDirective::Execute);
    }

    #[tokio::test]
    async fn reflector_scores_usefulness_deterministically() {
        let reflector = HeuristicReflector;
        let g = graph(vec![], vec![]);
        let useful = reflector
            .reflect(&g, &proposal("tcp_scan", "127.0.0.1", "a1"), &tool_result(ToolExecutionStatus::Succeeded, 2))
            .await
            .unwrap();
        assert!(useful.useful);
        assert!(!useful.retry);

        let empty = reflector
            .reflect(&g, &proposal("tcp_scan", "127.0.0.1", "a1"), &tool_result(ToolExecutionStatus::Succeeded, 0))
            .await
            .unwrap();
        assert!(!empty.useful);
        assert!(empty.retry);

        let failed = reflector
            .reflect(&g, &proposal("tcp_scan", "127.0.0.1", "a1"), &tool_result(ToolExecutionStatus::Failed, 0))
            .await
            .unwrap();
        assert!(failed.reason_codes.contains(&"TOOL_FAILED".to_string()));
        assert_eq!(failed.lessons.len(), 1);
    }

    #[tokio::test]
    async fn replanner_retries_then_falls_back_to_alternative_tool() {
        let replanner = RuleReplanner;
        let reflection = ActionReflection {
            reflection_id: "r1".into(),
            action_id: "a-1".into(),
            useful: false,
            information_gain: 0.0,
            tool_selection_quality: 0.0,
            result_quality: 0.0,
            retry: true,
            reason_codes: vec!["TOOL_FAILED".into()],
            lessons: vec![],
        };
        // Attempts remaining → L1 retry with bumped attempt.
        let g = graph(
            vec![node("a1", PlanNodeType::Action, PlanNodeStatus::Pending, 1, None, Some("tcp_scan"), Some("127.0.0.1"), 0, 2)],
            vec![],
        );
        let meta = kairos_platform_pent_core::cognition::evaluate_meta_state(
            "run", "plan", 1,
            MetaMetrics {
                plan_confidence: 1.0, observation_confidence: 1.0, evidence_coverage: 0.0,
                recent_information_gain: 0.0, loop_similarity: 0.0, failure_streak: 0,
                budget_pressure: 0.0, scope_risk: 0.0, model_disagreement: 0.0,
            },
        );
        let delta = replanner
            .replan(&g, &proposal("tcp_scan", "127.0.0.1", "a1"), &tool_result(ToolExecutionStatus::Failed, 0), &reflection, &meta)
            .await
            .unwrap()
            .expect("retry delta");
        assert_eq!(delta.replan_level, ReplanLevel::Parameter);
        let next = kairos_platform_pent_core::cognition::apply_plan_delta(&g, &delta).unwrap();
        let retried = next.nodes.iter().find(|n| n.node_id == "a1").unwrap();
        assert_eq!(retried.attempt, 1);
        assert_eq!(retried.status, PlanNodeStatus::Pending);

        // Attempts exhausted → L2: node failed + alternative action added.
        let g2 = graph(
            vec![node("a1", PlanNodeType::Action, PlanNodeStatus::Pending, 1, Some("s1"), Some("tcp_scan"), Some("127.0.0.1"), 1, 2)],
            vec![],
        );
        let delta2 = replanner
            .replan(&g2, &proposal("tcp_scan", "127.0.0.1", "a1"), &tool_result(ToolExecutionStatus::Failed, 0), &reflection, &meta)
            .await
            .unwrap()
            .expect("fallback delta");
        assert_eq!(delta2.replan_level, ReplanLevel::Tool);
        let next2 = kairos_platform_pent_core::cognition::apply_plan_delta(&g2, &delta2).unwrap();
        assert_eq!(
            next2.nodes.iter().find(|n| n.node_id == "a1").unwrap().status,
            PlanNodeStatus::Failed
        );
        let alt = next2.nodes.iter().find(|n| n.node_id == "a1-alt").expect("alt node");
        assert_eq!(alt.capability.as_deref(), Some("http_probe"));
        assert_eq!(alt.status, PlanNodeStatus::Pending);
    }
}
