use kairos_platform_framework_database::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedRun {
    pub id: String,
    #[allow(dead_code)] // required by the tenant-aware claim row mapping
    pub tenant_id: Option<i64>,
    pub lease_ttl_seconds: i32,
    pub max_retries_per_run: i32,
    pub retry_base_delay_seconds: i32,
    pub max_duration_seconds: i32,
}

/// Claim a queued run for a worker. Uses advisory locking and row-level
/// concurrency control to ensure exactly-once dispatch.
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
