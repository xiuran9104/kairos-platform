//! Cognition engine driver — runs a claimed Run through the H-PER-M cycle:
//! plan bootstrap → (stage expansion → meta → step → preflect → execute →
//! reflect → replan) until the plan is terminal, the meta monitor intervenes,
//! or a hard guard trips. All status transitions are PlanDeltas.

use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::cognition::*;
use serde_json::json;

use super::adapters::{
    DbMetaMetrics, DeterministicStepPlanner, HeuristicReflector, ProcessExecutor, RuleReplanner,
    RunEventSink, ScopePreflector, select_all_executable_actions, select_all_expandable_stages,
    select_next_action,
};
use super::plan_repository_postgres::PgPlanRepository;
use super::ports::PlanRepository;
use super::runtime::CognitionRuntime;
use std::sync::Arc;

const MAX_CONSECUTIVE_NO_PROGRESS: u32 = 3;

/// Execute a task-only Run with the cognition engine. The caller (worker)
/// owns claiming, leasing and failure handling; this function drives the
/// plan to a terminal state and writes the deterministic report on success.
pub async fn execute_cognition_run(
    pool: &PgPool,
    run_id: &str,
    worker_id: &str,
) -> Result<(), String> {
    execute_cognition_run_with_backend(pool, run_id, worker_id, None).await
}

/// Same as [`execute_cognition_run`] with an explicit executor backend
/// override (used by integration tests to run on the host backend).
pub async fn execute_cognition_run_with_backend(
    pool: &PgPool,
    run_id: &str,
    worker_id: &str,
    backend: Option<crate::infrastructure::process::ExecutionBackend>,
) -> Result<(), String> {
    let run: Option<(Option<String>, serde_json::Value, i64)> = sqlx::query_as(
        "SELECT objective_snapshot, scope_snapshot, tenant_id FROM pent_runs WHERE id=$1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let (objective, scope_snapshot, tenant_id) =
        run.ok_or_else(|| format!("Run {run_id} 不存在"))?;
    let targets: Vec<String> = scope_snapshot
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter_map(|target| target.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let Some(target) = targets.first().filter(|target| !target.trim().is_empty()) else {
        return Err(format!("Run {run_id} 缺少已确认的不可变目标范围"));
    };

    let plans = Arc::new(PgPlanRepository::new(pool.clone()));
    // Bootstrap the initial plan: try LLM meta-planner first; on any failure
    // (model unavailable, invalid output, etc.) fall back to the deterministic
    // template transparently.
    let mut graph = match plans.current(run_id).await {
        Ok(graph) => graph,
        Err(_) => {
            let (mut initial, plan_source) = super::meta_planner::MetaPlanner::plan_with_llm(
                pool,
                run_id,
                target,
                objective.as_deref(),
                &scope_snapshot,
                tenant_id,
            )
            .await;
            if let Some(objective) = objective.filter(|value| !value.trim().is_empty()) {
                initial.objective = objective;
            }
            plans
                .save_initial(&initial)
                .await
                .map_err(|error| error.to_string())?;
            crate::append_run_event(
                pool,
                run_id,
                "run.plan.created",
                json!({
                    "plan_id": initial.plan_id,
                    "source": plan_source,
                    "worker_id": worker_id,
                    "stages": initial.nodes.len(),
                }),
            )
            .await?;
            initial
        }
    };

    let max_cycles = graph.global_budget.max_actions.max(1);
    let max_concurrency = graph.global_budget.max_concurrency.max(1) as usize;
    let mut no_progress = 0u32;
    for _cycle in 0..max_cycles {
        if crate::run_cancel_requested(pool, run_id).await? {
            return Err("RUN_CANCELLED".into());
        }

        // ── Stage expansion (parallel-aware) ──────────────────────────
        // Expand ALL unblocked stages at once. Independent stages (no
        // Requires edges between each other) will have their actions
        // available concurrently in the next step.
        if select_next_action(&graph).is_none() {
            let stage_ids: Vec<(String, String)> = select_all_expandable_stages(&graph)
                .into_iter()
                .map(|s| (s.node_id.clone(), s.objective.clone()))
                .collect();
            if !stage_ids.is_empty() {
                let mut any_expanded = false;
                for (stage_id, _objective) in stage_ids {
                    // Re-fetch stage from current graph after each delta
                    let stage = match graph.nodes.iter().find(|n| n.node_id == stage_id) {
                        Some(s) => s.clone(),
                        None => continue,
                    };
                    let children =
                        super::stage_planner::StagePlanner::expand_stage(&stage, &[]);
                    if children.is_empty() {
                        graph = apply_driver_delta(
                            pool,
                            &plans,
                            &graph,
                            ReplanLevel::Stage,
                            &["STAGE_NO_ACTIONS"],
                            vec![PlanDeltaOperation::UpdateNode {
                                node_id: stage_id,
                                patch: json!({"status": "completed"}),
                            }],
                        )
                        .await?;
                        any_expanded = true;
                        continue;
                    }
                    let mut operations = vec![PlanDeltaOperation::UpdateNode {
                        node_id: stage_id.clone(),
                        patch: json!({"status": "running"}),
                    }];
                    operations.extend(
                        children
                            .into_iter()
                            .map(|node| PlanDeltaOperation::AddNode { node }),
                    );
                    graph = apply_driver_delta(
                        pool,
                        &plans,
                        &graph,
                        ReplanLevel::Stage,
                        &["STAGE_EXPAND"],
                        operations,
                    )
                    .await?;
                    any_expanded = true;
                }
                if any_expanded {
                    continue;
                }
            } else {
                break; // no expandable stages → plan terminal
            }
        }

        // ── Parallel action execution (controlled fan-out) ───────────
        // Pick up to max_concurrency actions across all Running stages
        // and dispatch them concurrently via tokio tasks.
        let action_ids: Vec<String> = {
            let candidates = select_all_executable_actions(&graph);
            if candidates.is_empty() {
                no_progress += 1;
                if no_progress >= MAX_CONSECUTIVE_NO_PROGRESS {
                    return finish_blocked(
                        pool,
                        run_id,
                        &graph,
                        "no_executable_action",
                        "计划中没有可执行动作且无法展开新阶段",
                    )
                    .await;
                }
                continue;
            }
            candidates
                .into_iter()
                .take(max_concurrency)
                .map(|node| node.node_id.clone())
                .collect()
        };
        let concurrency = action_ids.len();

        // Spawn each action in its own task.
        let mut joinset = tokio::task::JoinSet::new();
        for node_id in action_ids {
            let runtime = CognitionRuntime::new(
                plans.clone(),
                Arc::new(DeterministicStepPlanner),
                Arc::new(ScopePreflector::new(&scope_snapshot)),
                Arc::new(ProcessExecutor::new(pool.clone(), run_id)),
                Arc::new(HeuristicReflector),
                Arc::new(RuleReplanner),
                Arc::new(DbMetaMetrics::new(pool.clone())),
                Arc::new(RunEventSink::new(pool.clone(), run_id)),
            );
            let run_id = run_id.to_string();
            joinset.spawn(async move {
                let cycle = runtime.run_cycle_for_action(&run_id, &node_id).await;
                (node_id, cycle)
            });
        }

        // Collect results; each task operates on the shared plan via DB.
        let mut made_progress = false;
        while let Some(result) = joinset.join_next().await {
            match result {
                Ok((_node_id, Ok(cycle))) => {
                    let current = plans
                        .current(run_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    match cycle.intervention {
                        MetaIntervention::Continue => {
                            graph = apply_outcome_deltas(pool, &plans, current, &cycle)
                                .await?;
                            made_progress = true;
                        }
                        MetaIntervention::Stop | MetaIntervention::Pause
                        | MetaIntervention::AskUser => {
                            // Terminal intervention from any sub-agent stops
                            // the whole run.
                            let reason = match cycle.intervention {
                                MetaIntervention::Stop => ("meta_stop", "Scope 风险"),
                                MetaIntervention::Pause => ("budget_pressure", "预算压力"),
                                _ => ("needs_user_input", "需要用户输入"),
                            };
                            return finish_blocked(
                                pool, run_id, &current, reason.0, reason.1,
                            )
                            .await;
                        }
                        _ => {
                            // Replan / GatherInformation / RequestApproval:
                            // mark action appropriately so it doesn't loop.
                            if let (Some(proposal), Some(preflection)) =
                                (&cycle.proposal, &cycle.preflection)
                            {
                                let status = match preflection.directive {
                                    PreFlectDirective::Replace => "skipped",
                                    PreFlectDirective::Block => "blocked",
                                    _ => "skipped",
                                };
                                graph = apply_driver_delta(
                                    pool,
                                    &plans,
                                    &current,
                                    ReplanLevel::Stage,
                                    &["PREFLECT_NON_EXECUTE"],
                                    vec![PlanDeltaOperation::UpdateNode {
                                        node_id: proposal.node_id.clone(),
                                        patch: json!({"status": status}),
                                    }],
                                )
                                .await?;
                                graph = apply_outcome_deltas(pool, &plans, graph, &cycle)
                                    .await?;
                                made_progress = true;
                            }
                        }
                    }
                }
                Ok((_, Err(error))) => {
                    if error.to_string().contains("NO_EXECUTABLE_ACTION") {
                        continue; // this action was stale; other tasks may succeed
                    }
                    return Err(error.to_string());
                }
                Err(join_error) => {
                    return Err(format!("sub-agent task panicked: {join_error}"));
                }
            }
        }

        if made_progress {
            no_progress = 0;
        }
        graph = plans.current(run_id).await.map_err(|error| error.to_string())?;
        if plan_terminal(&graph) {
            break;
        }
        tracing::debug!(
            concurrency,
            run_id = %run_id,
            "parallel action batch completed"
        );
    }

    if !plan_terminal(&graph) {
        return finish_blocked(
            pool,
            run_id,
            &graph,
            "plan_incomplete",
            "计划未在预算周期内执行完毕",
        )
        .await;
    }
    finish_completed(pool, run_id, &graph).await
}

fn plan_terminal(graph: &TaskCoordinationGraph) -> bool {
    graph
        .nodes
        .iter()
        .filter(|node| node.node_type == PlanNodeType::Stage)
        .all(|stage| {
            matches!(
                stage.status,
                PlanNodeStatus::Completed
                    | PlanNodeStatus::Failed
                    | PlanNodeStatus::Skipped
                    | PlanNodeStatus::Blocked
            )
        })
}

async fn apply_driver_delta(
    pool: &PgPool,
    plans: &PgPlanRepository,
    graph: &TaskCoordinationGraph,
    level: ReplanLevel,
    reasons: &[&str],
    operations: Vec<PlanDeltaOperation>,
) -> Result<TaskCoordinationGraph, String> {
    if operations.is_empty() {
        return Ok(graph.clone());
    }
    let delta = PlanDelta {
        delta_id: uuid::Uuid::new_v4().to_string(),
        plan_id: graph.plan_id.clone(),
        base_version: graph.version,
        replan_level: level,
        reason_codes: reasons.iter().map(|reason| reason.to_string()).collect(),
        operations,
        proposed_by: "cognition-driver".into(),
        created_at: chrono::Utc::now(),
    };
    let next = plans
        .apply_delta(graph, &delta)
        .await
        .map_err(|error| error.to_string())?;
    crate::append_run_event(
        pool,
        &next.run_id,
        "plan.delta.applied",
        json!({
            "delta_id": delta.delta_id,
            "level": delta.replan_level,
            "reasons": delta.reason_codes,
            "version": next.version,
        }),
    )
    .await?;
    Ok(next)
}

/// Persist the outcome of one executed action: mark the action Completed on
/// useful results, then roll up stage statuses and skip dead-end stages.
async fn apply_outcome_deltas(
    pool: &PgPool,
    plans: &PgPlanRepository,
    mut graph: TaskCoordinationGraph,
    cycle: &super::runtime::CognitionCycleResult,
) -> Result<TaskCoordinationGraph, String> {
    if cycle.reflection.as_ref().is_some_and(|r| r.useful)
        && let Some(proposal) = &cycle.proposal
    {
        graph = apply_driver_delta(
            pool,
            plans,
            &graph,
            ReplanLevel::Stage,
            &["ACTION_COMPLETED"],
            vec![PlanDeltaOperation::UpdateNode {
                node_id: proposal.node_id.clone(),
                patch: json!({"status": "completed"}),
            }],
        )
        .await?;
    }
    // Stage rollup: a Running stage whose children are all terminal becomes
    // Completed (any child completed) or Failed (none did).
    let stages: Vec<String> = graph
        .nodes
        .iter()
        .filter(|node| node.node_type == PlanNodeType::Stage)
        .map(|node| node.node_id.clone())
        .collect();
    let mut rollup = Vec::new();
    for stage_id in stages {
        let Some(stage) = graph.nodes.iter().find(|node| node.node_id == stage_id) else {
            continue;
        };
        let mut children = graph
            .nodes
            .iter()
            .filter(|node| node.parent_id.as_deref() == Some(stage.node_id.as_str()))
            .map(|node| node.status.clone())
            .collect::<Vec<_>>();
        if children.is_empty() {
            // Unexpanded pending stage with a failed dependency → skip it.
            if stage.status == PlanNodeStatus::Pending {
                let dep_failed = graph
                    .edges
                    .iter()
                    .filter(|edge| {
                        edge.to == stage.node_id && edge.edge_type == PlanEdgeType::Requires
                    })
                    .any(|edge| {
                        graph
                            .nodes
                            .iter()
                            .find(|node| node.node_id == edge.from)
                            .is_some_and(|dep| {
                                matches!(
                                    dep.status,
                                    PlanNodeStatus::Failed | PlanNodeStatus::Skipped
                                )
                            })
                    });
                if dep_failed {
                    rollup.push(PlanDeltaOperation::UpdateNode {
                        node_id: stage.node_id.clone(),
                        patch: json!({"status": "skipped"}),
                    });
                }
            }
            continue;
        }
        if stage.status != PlanNodeStatus::Running {
            continue;
        }
        let terminal = children.iter().all(|status| {
            matches!(
                status,
                PlanNodeStatus::Completed
                    | PlanNodeStatus::Failed
                    | PlanNodeStatus::Skipped
                    | PlanNodeStatus::Blocked
            )
        });
        if !terminal {
            continue;
        }
        let any_completed = children
            .drain(..)
            .any(|status| status == PlanNodeStatus::Completed);
        rollup.push(PlanDeltaOperation::UpdateNode {
            node_id: stage.node_id.clone(),
            patch: json!({"status": if any_completed { "completed" } else { "failed" }}),
        });
    }
    if !rollup.is_empty() {
        graph = apply_driver_delta(pool, plans, &graph, ReplanLevel::Stage, &["STAGE_ROLLUP"], rollup).await?;
    }
    Ok(graph)
}

async fn finish_blocked(
    pool: &PgPool,
    run_id: &str,
    graph: &TaskCoordinationGraph,
    code: &str,
    message: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE pent_runs
         SET status='blocked', failure_code=$2, failure_message=$3,
             completed_at=now(), lease_expires_at=NULL, updated_at=now()
         WHERE id=$1",
    )
    .bind(run_id)
    .bind(code)
    .bind(message)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    crate::append_run_event(
        pool,
        run_id,
        "run.blocked",
        json!({"reason": code, "message": message, "plan_version": graph.version}),
    )
    .await?;
    Ok(())
}

async fn finish_completed(
    pool: &PgPool,
    run_id: &str,
    graph: &TaskCoordinationGraph,
) -> Result<(), String> {
    let completed_stages = graph
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == PlanNodeType::Stage && node.status == PlanNodeStatus::Completed
        })
        .count();
    sqlx::query(
        "UPDATE pent_runs SET status='completed', completed_at=now(), updated_at=now()
         WHERE id=$1",
    )
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    crate::append_run_event(
        pool,
        run_id,
        "run.completed",
        json!({
            "engine": "cognition",
            "plan_id": graph.plan_id,
            "plan_version": graph.version,
            "stages_completed": completed_stages,
        }),
    )
    .await?;
    match crate::report::generate_run_report(pool, run_id, None).await {
        Ok(artifact) => {
            crate::append_run_event(
                pool,
                run_id,
                "run.report.completed",
                json!({
                    "artifact_id": artifact.artifact_id,
                    "object_key": artifact.object_key,
                    "size_bytes": artifact.size_bytes,
                    "sha256": artifact.sha256,
                }),
            )
            .await?;
        }
        Err(error) => {
            tracing::error!(run_id = %run_id, error = %error, "run report generation failed");
            crate::append_run_event(pool, run_id, "run.report.failed", json!({"error": error}))
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::infrastructure::process::ExecutionBackend;

    #[tokio::test]
    #[ignore = "requires local PostgreSQL (TEST_DATABASE_URL), MinIO and /usr/bin/nmap"]
    async fn cognition_engine_completes_run_end_to_end() {
        if !std::path::Path::new("/usr/bin/nmap").exists() {
            return;
        }
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        let pool = kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database");
        let run_id = "cognition-it-run";

        // Clean any previous residue.
        for statement in [
            "DELETE FROM pent_reflections WHERE run_id=$1",
            "DELETE FROM pent_pre_reflections WHERE run_id=$1",
            "DELETE FROM pent_metacognitive_snapshots WHERE run_id=$1",
            "DELETE FROM pent_plan_deltas WHERE run_id=$1",
            "DELETE FROM pent_plan_versions WHERE run_id=$1",
            "DELETE FROM pent_artifacts WHERE run_id=$1",
            "DELETE FROM pent_evidence WHERE run_id=$1",
            "DELETE FROM pent_tool_calls WHERE run_id=$1",
            "DELETE FROM pent_run_events WHERE run_id=$1",
            "DELETE FROM pent_runs WHERE id=$1",
        ] {
            sqlx::query(statement)
                .bind(run_id)
                .execute(&pool)
                .await
                .expect("cleanup residue");
        }
        sqlx::query(
            "INSERT INTO pent_runs(id, task_id, tenant_id, user_id, status,
                                   objective_snapshot, scope_snapshot, budget_snapshot)
             VALUES ($1,'cognition-it-task',0,'it-tester','running',
                     '对 127.0.0.1 进行授权安全测试',
                     '{\"targets\":[\"127.0.0.1\"],\"confirmed\":true}'::jsonb,
                     '{\"max_tool_calls\":30}'::jsonb)",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed run");

        let result = super::execute_cognition_run_with_backend(
            &pool,
            run_id,
            "it-worker",
            Some(ExecutionBackend::Host),
        )
        .await;
        assert!(result.is_ok(), "认知引擎必须完成，实际：{result:?}");

        let status: String = sqlx::query_scalar("SELECT status FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .expect("query run status");
        assert_eq!(status, "completed");

        let plan_versions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_plan_versions WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("count plan versions");
        assert!(plan_versions >= 2, "计划必须有版本演进，实际 {plan_versions}");
        let deltas: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_plan_deltas WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("count deltas");
        assert!(deltas >= 1, "必须存在 PlanDelta 记录");

        let executed_tools: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT tool_name FROM pent_tool_calls WHERE run_id=$1",
        )
        .bind(run_id)
        .fetch_all(&pool)
        .await
        .expect("list executed tools");
        assert!(
            executed_tools.iter().any(|name| name == "tcp_scan"),
            "tcp_scan 必须执行，实际：{executed_tools:?}"
        );
        let evidence_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_evidence WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("count evidence");
        assert!(evidence_count >= 1, "执行必须留存证据");
        let reflections: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_reflections WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("count reflections");
        assert!(reflections >= 1, "每个动作必须产生反思记录");
        let snapshots: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pent_metacognitive_snapshots WHERE run_id=$1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("count meta snapshots");
        assert!(snapshots >= 1, "元认知快照必须持久化");

        let report_event: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pent_run_events
             WHERE run_id=$1 AND event_type='run.report.completed')",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("query report event");
        assert!(report_event, "完成后必须生成确定性报告");
        let artifact_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pent_artifacts WHERE run_id=$1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("count artifacts");
        assert_eq!(artifact_count, 1);

        // Final plan state: every stage terminal.
        let graph: serde_json::Value = sqlx::query_scalar(
            "SELECT graph FROM pent_plan_versions WHERE run_id=$1
             ORDER BY version DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("load final graph");
        let stage_statuses: Vec<&str> = graph["nodes"]
            .as_array()
            .expect("nodes array")
            .iter()
            .filter(|node| node["node_type"] == "stage")
            .filter_map(|node| node["status"].as_str())
            .collect();
        assert!(
            stage_statuses
                .iter()
                .all(|status| matches!(*status, "completed" | "failed" | "skipped")),
            "所有阶段必须到达终态，实际：{stage_statuses:?}"
        );

        for statement in [
            "DELETE FROM pent_reflections WHERE run_id=$1",
            "DELETE FROM pent_pre_reflections WHERE run_id=$1",
            "DELETE FROM pent_metacognitive_snapshots WHERE run_id=$1",
            "DELETE FROM pent_plan_deltas WHERE run_id=$1",
            "DELETE FROM pent_plan_versions WHERE run_id=$1",
            "DELETE FROM pent_artifacts WHERE run_id=$1",
            "DELETE FROM pent_evidence WHERE run_id=$1",
            "DELETE FROM pent_tool_calls WHERE run_id=$1",
            "DELETE FROM pent_run_events WHERE run_id=$1",
            "DELETE FROM pent_runs WHERE id=$1",
        ] {
            sqlx::query(statement)
                .bind(run_id)
                .execute(&pool)
                .await
                .expect("cleanup after test");
        }
    }
}
