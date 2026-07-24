//! HPM-11: Memory adapter — cognition memory adapter.
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
        sqlx::query_scalar::<_, String>("SELECT content FROM pent_agent_lessons WHERE status='promoted' AND content ILIKE $1 LIMIT $2")
            .bind(format!("%{}%", fingerprint)).bind(limit as i64).fetch_all(&self.pool).await.unwrap_or_default()
    }
}
