//! LLM-08: Memory adapter — RMOS recall + citation support.
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::ConversationCitation;

pub struct MemoryAdapter {
    pool: PgPool,
}
impl MemoryAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl MemoryAdapter {
    /// Recall relevant memory entries for a given context.
    pub async fn recall(&self, query: &str, limit: usize) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT content FROM pent_knowledge_documents WHERE to_tsvector('simple', content) @@ plainto_tsquery('simple', $1) ORDER BY ts_rank(to_tsvector('simple', content), plainto_tsquery('simple', $1)) DESC LIMIT $2"
        ).bind(query).bind(limit as i64).fetch_all(&self.pool).await.unwrap_or_default()
    }

    /// Build citations from retrieved documents.
    pub fn build_citations(docs: &[String]) -> Vec<ConversationCitation> {
        docs.iter()
            .enumerate()
            .map(|(i, doc)| ConversationCitation {
                kind: "memory".into(),
                reference_id: format!("mem-{}", i),
                label: doc.chars().take(80).collect(),
            })
            .collect()
    }
}
