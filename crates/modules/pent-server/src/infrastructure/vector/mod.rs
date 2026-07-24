//! Vector index for tool search (AI-05). pgvector backend.
use kairos_platform_framework_database::PgPool;

pub struct PgVectorIndex {
    pool: PgPool,
}

impl PgVectorIndex {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(
        &self,
        tool_id: &str,
        version: &str,
        embedding: &[f32],
    ) -> Result<(), String> {
        let emb_str = format!(
            "[{}]",
            embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        sqlx::query("INSERT INTO pent_tool_definitions(tool_id, version, namespace, status, definition) VALUES($1, $2, 'default', 'active', jsonb_build_object('embedding', $3::vector)) ON CONFLICT(tool_id, version) DO UPDATE SET definition = pent_tool_definitions.definition || jsonb_build_object('embedding', $3::vector), updated_at = now()")
            .bind(tool_id).bind(version).bind(&emb_str)
            .execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn delete(&self, tool_id: &str, version: &str) -> Result<(), String> {
        sqlx::query("UPDATE pent_tool_definitions SET definition = definition - 'embedding', updated_at = now() WHERE tool_id = $1 AND version = $2")
            .bind(tool_id).bind(version).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        namespace: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<VectorSearchResult>, String> {
        let emb_str = format!(
            "[{}]",
            query_embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let rows = sqlx::query_as::<_, (String, String, f64)>(
            "SELECT tool_id, version, 1 - (definition->>'embedding')::vector <=> $1::vector as similarity FROM pent_tool_definitions WHERE deleted = false AND status = 'active' AND ($2::text IS NULL OR namespace = $2) AND definition ? 'embedding' ORDER BY similarity LIMIT $3"
        ).bind(&emb_str).bind(namespace).bind(top_k as i64).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|(id, ver, score)| VectorSearchResult {
                tool_id: id,
                version: ver,
                score: score as f32,
            })
            .collect())
    }

    pub async fn health(&self) -> Result<bool, String> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| true)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VectorSearchResult {
    pub tool_id: String,
    pub version: String,
    pub score: f32,
}
