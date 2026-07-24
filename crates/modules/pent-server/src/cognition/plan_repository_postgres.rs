//! HPM-01: PostgreSQL PlanRepository — versioned TCG persistence.
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::cognition::{PlanDelta, TaskCoordinationGraph};

pub struct PgPlanRepository {
    pool: PgPool,
}
impl PgPlanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PgPlanRepository {
    pub async fn save_initial(
        &self,
        run_id: &str,
        tcg: &TaskCoordinationGraph,
    ) -> anyhow::Result<i64> {
        let tcg_json = serde_json::to_value(tcg)?;
        let version: i64 = sqlx::query_scalar(
            "INSERT INTO pent_plan_versions(run_id,version,plan_data,status,created_at) VALUES($1,1,$2,'active',now()) RETURNING version"
        ).bind(run_id).bind(&tcg_json).fetch_one(&self.pool).await?;
        Ok(version)
    }

    pub async fn current(&self, run_id: &str) -> anyhow::Result<TaskCoordinationGraph> {
        let data: serde_json::Value = sqlx::query_scalar(
            "SELECT plan_data FROM pent_plan_versions WHERE run_id=$1 AND status='active' ORDER BY version DESC LIMIT 1"
        ).bind(run_id).fetch_optional(&self.pool).await?.ok_or_else(|| anyhow::anyhow!("no active plan"))?;
        Ok(serde_json::from_value(data)?)
    }

    pub async fn apply_delta(&self, run_id: &str, delta: &PlanDelta) -> anyhow::Result<i64> {
        let current_ver: i64 = sqlx::query_scalar("SELECT version FROM pent_plan_versions WHERE run_id=$1 AND status='active' ORDER BY version DESC LIMIT 1")
            .bind(run_id).fetch_optional(&self.pool).await?.unwrap_or(0);
        if delta.base_version != current_ver {
            anyhow::bail!(
                "PLAN_VERSION_CONFLICT: base={} current={}",
                delta.base_version,
                current_ver
            );
        }
        let delta_json = serde_json::to_value(delta)?;
        let new_ver: i64 = sqlx::query_scalar(
            "INSERT INTO pent_plan_versions(run_id,version,plan_data,status,created_at) SELECT $1,version+1,plan_data,'active',now() FROM pent_plan_versions WHERE run_id=$1 AND status='active' ORDER BY version DESC LIMIT 1 RETURNING version"
        ).bind(run_id).fetch_one(&self.pool).await?;
        sqlx::query("INSERT INTO pent_plan_deltas(run_id,from_version,to_version,delta_data,status,created_at) VALUES($1,$2,$3,$4,'applied',now())")
            .bind(run_id).bind(current_ver).bind(new_ver).bind(&delta_json).execute(&self.pool).await?;
        Ok(new_ver)
    }
}
