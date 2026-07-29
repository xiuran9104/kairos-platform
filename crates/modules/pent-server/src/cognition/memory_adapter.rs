//! HPM-11: Memory adapter — recall promoted lessons for the cognition
//! runtime. Only `promoted` lessons are eligible (they passed validation).
use kairos_platform_framework_database::PgPool;

pub struct CognitionMemoryAdapter {
    pool: PgPool,
}

impl CognitionMemoryAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl CognitionMemoryAdapter {
    pub async fn recall_similar(&self, fingerprint: &str, limit: usize) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT lesson FROM pent_agent_lessons
             WHERE status='promoted' AND lesson ILIKE $1
             ORDER BY validation_count DESC, updated_at DESC
             LIMIT $2",
        )
        .bind(format!("%{}%", fingerprint))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
    }
}
