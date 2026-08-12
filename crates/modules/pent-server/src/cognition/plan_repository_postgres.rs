//! HPM-01: PostgreSQL PlanRepository — versioned TCG persistence against the
//! real `pent_plan_versions` / `pent_plan_deltas` schema. All graph mutation
//! goes through `pent-core::cognition` (validate + apply delta) so the
//! database only ever stores valid, replayable plan versions.

use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::cognition::{
    ActionReflection, MetaCognitiveSnapshot, PlanDelta, PreFlection, TaskCoordinationGraph,
};
use sha2::{Digest, Sha256};

use super::ports::PlanRepository;

pub struct PgPlanRepository {
    pool: PgPool,
}

impl PgPlanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn graph_hash(graph: &TaskCoordinationGraph) -> anyhow::Result<String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_string(graph)?.as_bytes())
        ))
    }
}

#[async_trait]
impl PlanRepository for PgPlanRepository {
    async fn current(&self, run_id: &str) -> anyhow::Result<TaskCoordinationGraph> {
        let data: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT graph FROM pent_plan_versions
             WHERE run_id=$1 ORDER BY version DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        let data = data.ok_or_else(|| anyhow::anyhow!("run {run_id} 不存在计划版本"))?;
        Ok(serde_json::from_value(data)?)
    }

    async fn save_initial(&self, graph: &TaskCoordinationGraph) -> anyhow::Result<()> {
        kairos_platform_pent_core::cognition::validate_graph(graph)
            .map_err(|errors| anyhow::anyhow!("初始计划图非法：{}", errors.join(",")))?;
        sqlx::query(
            "INSERT INTO pent_plan_versions(
                 plan_id, run_id, version, objective, graph, content_hash, created_by, created_at
             ) VALUES($1,$2,$3,$4,$5,$6,'cognition-engine',now())",
        )
        .bind(&graph.plan_id)
        .bind(&graph.run_id)
        .bind(graph.version)
        .bind(&graph.objective)
        .bind(serde_json::to_value(graph)?)
        .bind(Self::graph_hash(graph)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn apply_delta(
        &self,
        current: &TaskCoordinationGraph,
        delta: &PlanDelta,
    ) -> anyhow::Result<TaskCoordinationGraph> {
        let next = kairos_platform_pent_core::cognition::apply_plan_delta(current, delta)
            .map_err(|errors| anyhow::anyhow!("计划增量应用失败：{}", errors.join(",")))?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pent_plan_versions(
                 plan_id, run_id, version, objective, graph, content_hash, created_by, created_at
             ) VALUES($1,$2,$3,$4,$5,$6,'cognition-engine',now())",
        )
        .bind(&next.plan_id)
        .bind(&next.run_id)
        .bind(next.version)
        .bind(&next.objective)
        .bind(serde_json::to_value(&next)?)
        .bind(Self::graph_hash(&next)?)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO pent_plan_deltas(
                 id, plan_id, run_id, base_version, result_version, replan_level,
                 reason_codes, operations, proposed_by, status, created_at, applied_at
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'applied',now(),now())",
        )
        .bind(&delta.delta_id)
        .bind(&delta.plan_id)
        .bind(&next.run_id)
        .bind(delta.base_version)
        .bind(next.version)
        .bind(
            serde_json::to_value(&delta.replan_level)?
                .as_str()
                .unwrap_or("parameter")
                .to_string(),
        )
        .bind(serde_json::to_value(&delta.reason_codes)?)
        .bind(serde_json::to_value(&delta.operations)?)
        .bind(if delta.proposed_by.is_empty() {
            "cognition-engine"
        } else {
            delta.proposed_by.as_str()
        })
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(next)
    }

    async fn record_meta(&self, snapshot: &MetaCognitiveSnapshot) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO pent_metacognitive_snapshots(
                 id, run_id, plan_id, plan_version, state, intervention,
                 metrics, reason_codes, created_at
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,now())",
        )
        .bind(&snapshot.snapshot_id)
        .bind(&snapshot.run_id)
        .bind(&snapshot.plan_id)
        .bind(snapshot.plan_version)
        .bind(
            serde_json::to_value(&snapshot.state)?
                .as_str()
                .unwrap_or("confident")
                .to_string(),
        )
        .bind(
            serde_json::to_value(&snapshot.intervention)?
                .as_str()
                .unwrap_or("continue")
                .to_string(),
        )
        .bind(serde_json::to_value(&snapshot.metrics)?)
        .bind(serde_json::to_value(&snapshot.reason_codes)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_preflection(
        &self,
        graph: &TaskCoordinationGraph,
        reflection: &PreFlection,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO pent_pre_reflections(
                 id, run_id, plan_id, plan_version, action_id, directive,
                 expected_information_gain, confidence, predicted_outcomes,
                 risk_codes, alternatives, requires_approval, reasoning_summary, created_at
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now())",
        )
        .bind(&reflection.reflection_id)
        .bind(&graph.run_id)
        .bind(&graph.plan_id)
        .bind(graph.version)
        .bind(&reflection.action_id)
        .bind(
            serde_json::to_value(&reflection.directive)?
                .as_str()
                .unwrap_or("execute")
                .to_string(),
        )
        .bind(reflection.expected_information_gain)
        .bind(reflection.confidence)
        .bind(serde_json::to_value(&reflection.predicted_outcomes)?)
        .bind(serde_json::to_value(&reflection.risk_codes)?)
        .bind(serde_json::to_value(&reflection.alternatives)?)
        .bind(reflection.requires_approval)
        .bind(&reflection.reasoning_summary)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_action_reflection(
        &self,
        graph: &TaskCoordinationGraph,
        reflection: &ActionReflection,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO pent_reflections(
                 id, run_id, plan_id, plan_version, reflection_level, action_id,
                 useful, information_gain, reason_codes, lessons, created_at
             ) VALUES($1,$2,$3,$4,'action',$5,$6,$7,$8,$9,now())",
        )
        .bind(&reflection.reflection_id)
        .bind(&graph.run_id)
        .bind(&graph.plan_id)
        .bind(graph.version)
        .bind(&reflection.action_id)
        .bind(reflection.useful)
        .bind(reflection.information_gain)
        .bind(serde_json::to_value(&reflection.reason_codes)?)
        .bind(serde_json::to_value(&reflection.lessons)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
