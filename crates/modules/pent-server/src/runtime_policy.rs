use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RuntimePolicy {
    pub tenant_id: i64,
    pub max_concurrent_runs: i32,
    pub lease_ttl_seconds: i32,
    pub max_tool_timeout_seconds: i32,
    pub max_retries_per_run: i32,
    pub retry_base_delay_seconds: i32,
    pub approval_ttl_seconds: i32,
    pub auto_approve_low_risk: bool,
    pub require_approval_for: Value,
    pub updated_by: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRuntimePolicy {
    pub max_concurrent_runs: i32,
    pub lease_ttl_seconds: i32,
    pub max_tool_timeout_seconds: i32,
    pub max_retries_per_run: i32,
    pub retry_base_delay_seconds: i32,
    pub approval_ttl_seconds: i32,
    pub auto_approve_low_risk: bool,
    pub require_approval_for: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedRun {
    pub id: String,
    pub tenant_id: i64,
    pub lease_ttl_seconds: i32,
    pub max_retries_per_run: i32,
    pub retry_base_delay_seconds: i32,
    pub max_duration_seconds: i32,
}

pub async fn load(pool: &PgPool, tenant_id: i64) -> Result<RuntimePolicy, AppError> {
    sqlx::query_as::<_, RuntimePolicy>(
        "SELECT tenant_id,max_concurrent_runs,lease_ttl_seconds,max_tool_timeout_seconds,
                max_retries_per_run,retry_base_delay_seconds,approval_ttl_seconds,
                auto_approve_low_risk,require_approval_for,updated_by,updated_at
         FROM pent_runtime_policies
         WHERE tenant_id IN ($1, 0)
         ORDER BY CASE WHEN tenant_id=$1 THEN 0 ELSE 1 END
         LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::internal("Pent 默认运行策略不存在"))
}

pub async fn save(
    pool: &PgPool,
    tenant_id: i64,
    user_id: &str,
    update: UpdateRuntimePolicy,
) -> Result<RuntimePolicy, AppError> {
    validate(&update)?;
    let approval_levels = serde_json::to_value(update.require_approval_for)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    sqlx::query_as::<_, RuntimePolicy>(
        "INSERT INTO pent_runtime_policies(
             tenant_id,max_concurrent_runs,lease_ttl_seconds,max_tool_timeout_seconds,
             max_retries_per_run,retry_base_delay_seconds,approval_ttl_seconds,
             auto_approve_low_risk,require_approval_for,updated_by,updated_at
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())
         ON CONFLICT(tenant_id) DO UPDATE SET
             max_concurrent_runs=excluded.max_concurrent_runs,
             lease_ttl_seconds=excluded.lease_ttl_seconds,
             max_tool_timeout_seconds=excluded.max_tool_timeout_seconds,
             max_retries_per_run=excluded.max_retries_per_run,
             retry_base_delay_seconds=excluded.retry_base_delay_seconds,
             approval_ttl_seconds=excluded.approval_ttl_seconds,
             auto_approve_low_risk=excluded.auto_approve_low_risk,
             require_approval_for=excluded.require_approval_for,
             updated_by=excluded.updated_by,
             updated_at=now()
         RETURNING tenant_id,max_concurrent_runs,lease_ttl_seconds,max_tool_timeout_seconds,
                   max_retries_per_run,retry_base_delay_seconds,approval_ttl_seconds,
                   auto_approve_low_risk,require_approval_for,updated_by,updated_at",
    )
    .bind(tenant_id)
    .bind(update.max_concurrent_runs)
    .bind(update.lease_ttl_seconds)
    .bind(update.max_tool_timeout_seconds)
    .bind(update.max_retries_per_run)
    .bind(update.retry_base_delay_seconds)
    .bind(update.approval_ttl_seconds)
    .bind(update.auto_approve_low_risk)
    .bind(approval_levels)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))
}

pub async fn claim(
    pool: &PgPool,
    worker_id: &str,
    fallback_lease_ttl: i64,
) -> Result<Option<ClaimedRun>, String> {
    sqlx::query_as::<_, ClaimedRun>(
        "WITH candidate AS (
             SELECT r.id,r.tenant_id,
                    COALESCE(tp.max_concurrent_runs,dp.max_concurrent_runs) max_concurrent_runs,
                    COALESCE(tp.lease_ttl_seconds,dp.lease_ttl_seconds,$2::integer) lease_ttl_seconds,
                    COALESCE(tp.max_retries_per_run,dp.max_retries_per_run) max_retries_per_run,
                    COALESCE(tp.retry_base_delay_seconds,dp.retry_base_delay_seconds) retry_base_delay_seconds,
                    LEAST(GREATEST(COALESCE((r.budget_snapshot->>'max_duration_seconds')::integer,7200),1),7200) max_duration_seconds
             FROM pent_runs r
             LEFT JOIN pent_runtime_policies tp ON tp.tenant_id=r.tenant_id
             LEFT JOIN pent_runtime_policies dp ON dp.tenant_id=0
             WHERE r.status='queued' AND r.next_attempt_at<=now()
               AND r.cancel_requested_at IS NULL
               AND pg_try_advisory_xact_lock(r.tenant_id)
               AND (
                   SELECT count(*) FROM pent_runs active
                   WHERE active.tenant_id=r.tenant_id AND active.status='running'
               ) < COALESCE(tp.max_concurrent_runs,dp.max_concurrent_runs,1)
             ORDER BY r.created_at
             FOR UPDATE OF r SKIP LOCKED
             LIMIT 1
         )
         UPDATE pent_runs r
         SET status='running',worker_id=$1,
             lease_expires_at=now()+(candidate.lease_ttl_seconds||' seconds')::interval,
             started_at=COALESCE(started_at,now()),updated_at=now(),
             policy_snapshot=policy_snapshot || jsonb_build_object(
                 'lease_ttl_seconds',candidate.lease_ttl_seconds,
                 'max_retries_per_run',candidate.max_retries_per_run,
                 'retry_base_delay_seconds',candidate.retry_base_delay_seconds
             )
         FROM candidate
         WHERE r.id=candidate.id
         RETURNING r.id,r.tenant_id,candidate.lease_ttl_seconds,
                   candidate.max_retries_per_run,candidate.retry_base_delay_seconds,
                   candidate.max_duration_seconds",
    )
    .bind(worker_id)
    .bind(fallback_lease_ttl.clamp(30, 3600) as i32)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())
}

fn validate(update: &UpdateRuntimePolicy) -> Result<(), AppError> {
    if !(1..=100).contains(&update.max_concurrent_runs)
        || !(30..=3600).contains(&update.lease_ttl_seconds)
        || !(10..=7200).contains(&update.max_tool_timeout_seconds)
        || !(0..=20).contains(&update.max_retries_per_run)
        || !(1..=3600).contains(&update.retry_base_delay_seconds)
        || !(60..=86400).contains(&update.approval_ttl_seconds)
    {
        return Err(AppError::bad_request("运行策略参数超出允许范围"));
    }
    if update.require_approval_for.is_empty()
        || update
            .require_approval_for
            .iter()
            .any(|level| !matches!(level.as_str(), "low" | "medium" | "high" | "critical"))
    {
        return Err(AppError::bad_request("审批风险级别无效"));
    }
    Ok(())
}
