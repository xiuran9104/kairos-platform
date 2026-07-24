//! PostgreSQL FTS tool search (AI-04).
//! Full-text search with namespace/capability filtering, Top-20 recall.

use kairos_platform_framework_database::PgPool;

pub struct PgToolSearch {
    pool: PgPool,
}

impl PgToolSearch {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Full-text search with BM25-like ranking via ts_rank.
    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        capability: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<ToolSearchDoc>, String> {
        let ts_query = query
            .split_whitespace()
            .map(|w| format!("{}:*", w))
            .collect::<Vec<_>>()
            .join(" & ");
        let sql = r#"
            SELECT td.tool_id, td.version, td.namespace, td.status,
                   ts_rank(to_tsvector('simple', coalesce(td.definition->>'description','') || ' ' || coalesce(td.definition->>'name','')), to_tsquery('simple', $1)) as rank,
                   ts_headline('simple', coalesce(td.definition->>'description',''), to_tsquery('simple', $1), 'MaxWords=30, MinWords=10') as snippet
            FROM pent_tool_definitions td
            WHERE td.deleted = false AND td.status = 'active'
              AND ($2::text IS NULL OR td.namespace = $2)
              AND ($3::text IS NULL OR td.definition->>'capability' = $3)
              AND to_tsvector('simple', coalesce(td.definition->>'description','') || ' ' || coalesce(td.definition->>'name','')) @@ to_tsquery('simple', $1)
            ORDER BY rank DESC LIMIT $4
        "#;
        let rows = sqlx::query_as::<_, (String, String, String, String, f32, String)>(sql)
            .bind(&ts_query)
            .bind(namespace)
            .bind(capability)
            .bind(top_k as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|(id, ver, ns, status, rank, snippet)| ToolSearchDoc {
                tool_id: id,
                version: ver,
                namespace: ns,
                status,
                score: rank,
                snippet,
            })
            .collect())
    }

    /// Update search document (re-index after tool definition update).
    pub async fn update_search_document(
        &self,
        _tool_id: &str,
        _version: &str,
    ) -> Result<(), String> {
        // PostgreSQL tsvector is computed on-the-fly, no explicit reindex needed
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSearchDoc {
    pub tool_id: String,
    pub version: String,
    pub namespace: String,
    pub status: String,
    pub score: f32,
    pub snippet: String,
}
