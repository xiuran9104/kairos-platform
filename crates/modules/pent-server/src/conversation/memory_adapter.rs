//! LLM-08: Memory adapter — RMOS recall + citation support.
use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::ConversationCitation;

use super::{ConversationKnowledgeSource, KnowledgeGuide};

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

#[async_trait]
impl ConversationKnowledgeSource for MemoryAdapter {
    async fn search_guides(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<KnowledgeGuide>> {
        let mut patterns: Vec<String> = query
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(character, ',' | '，' | ':' | '：' | '/' | '\\' | '(' | ')')
            })
            .map(str::trim)
            .filter(|token| token.chars().count() >= 2)
            .take(12)
            .map(|token| format!("%{token}%"))
            .collect();
        if patterns.is_empty() {
            patterns.push(format!("%{}%", query.trim()));
        }
        let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT id::text,title,source,content
             FROM pent_knowledge_documents
             WHERE doc_type='guide'
               AND (title ILIKE ANY($1::text[]) OR content ILIKE ANY($1::text[]))
             ORDER BY
               (
                 SELECT COUNT(*) * 10 FROM unnest($1::text[]) pattern
                 WHERE title ILIKE pattern
               ) + (
                 SELECT COUNT(*) FROM unnest($1::text[]) pattern
                 WHERE content ILIKE pattern
               ) DESC,
               updated_at DESC
             LIMIT $2",
        )
        .bind(&patterns)
        .bind(i64::try_from(limit.clamp(1, 8)).unwrap_or(8))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, title, source, content)| KnowledgeGuide {
                id,
                title,
                source,
                content: if content.chars().count() > 5_000 {
                    format!(
                        "{}\n...[Wiki 内容已截断]",
                        content.chars().take(5_000).collect::<String>()
                    )
                } else {
                    content
                },
            })
            .collect())
    }
}
