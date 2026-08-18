//! pgvector-backed tool index.
//!
//! Tool metadata remains in `pent_tool_definitions`; vectors are stored in the
//! shared `langchain_pg_embedding` table and linked through `vector_record_id`.

use kairos_platform_framework_database::PgPool;
use serde_json::json;
use uuid::Uuid;

pub struct PgVectorIndex {
    pool: PgPool,
}

impl PgVectorIndex {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn has_records(&self) -> Result<bool, String> {
        sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1
                 FROM pent_tool_definitions tool
                 JOIN langchain_pg_embedding embedding
                   ON embedding.id::text = tool.vector_record_id
                 WHERE tool.lifecycle_status='active'
             )",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| error.to_string())
    }

    pub async fn upsert(
        &self,
        tool_id: &str,
        version: &str,
        embedding: &[f32],
    ) -> Result<(), String> {
        let vector = format_vector(embedding);
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT vector_record_id FROM pent_tool_definitions
             WHERE tool_id=$1 AND version=$2",
        )
        .bind(tool_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| error.to_string())?
        .flatten();
        let record_id = existing
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4);
        let mut transaction = self.pool.begin().await.map_err(|error| error.to_string())?;
        sqlx::query(
            "INSERT INTO langchain_pg_embedding(
                 id, document, embedding, cmetadata, custom_id, updated_at
             ) VALUES($1,$2,$3::vector,$4,$5,now())
             ON CONFLICT(id) DO UPDATE SET
                 document=EXCLUDED.document,
                 embedding=EXCLUDED.embedding,
                 cmetadata=EXCLUDED.cmetadata,
                 custom_id=EXCLUDED.custom_id,
                 updated_at=now()",
        )
        .bind(record_id)
        .bind(format!("{tool_id} {version}"))
        .bind(&vector)
        .bind(json!({"kind":"tool","tool_id":tool_id,"version":version}))
        .bind(format!("tool:{tool_id}:{version}"))
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            "UPDATE pent_tool_definitions
             SET vector_record_id=$3, embedding_version=1, updated_at=now()
             WHERE tool_id=$1 AND version=$2",
        )
        .bind(tool_id)
        .bind(version)
        .bind(record_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn delete(&self, tool_id: &str, version: &str) -> Result<(), String> {
        let mut transaction = self.pool.begin().await.map_err(|error| error.to_string())?;
        sqlx::query(
            "DELETE FROM langchain_pg_embedding embedding
             USING pent_tool_definitions tool
             WHERE tool.tool_id=$1 AND tool.version=$2
               AND embedding.id::text=tool.vector_record_id",
        )
        .bind(tool_id)
        .bind(version)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            "UPDATE pent_tool_definitions
             SET vector_record_id=NULL, embedding_model=NULL,
                 embedding_version=NULL, updated_at=now()
             WHERE tool_id=$1 AND version=$2",
        )
        .bind(tool_id)
        .bind(version)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        namespace: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<VectorSearchResult>, String> {
        let vector = format_vector(query_embedding);
        let rows = sqlx::query_as::<_, (String, String, f64)>(
            "SELECT tool.tool_id, tool.version,
                    1 - (embedding.embedding <=> $1::vector) AS similarity
             FROM pent_tool_definitions tool
             JOIN langchain_pg_embedding embedding
               ON embedding.id::text=tool.vector_record_id
             WHERE tool.lifecycle_status='active'
               AND ($2::text IS NULL OR tool.namespace=$2)
             ORDER BY similarity DESC
             LIMIT $3",
        )
        .bind(&vector)
        .bind(namespace)
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        Ok(rows
            .into_iter()
            .map(|(tool_id, version, score)| VectorSearchResult {
                tool_id,
                version,
                score: score as f32,
            })
            .collect())
    }

    pub async fn health(&self) -> Result<bool, String> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| true)
            .map_err(|error| error.to_string())
    }
}

fn format_vector(embedding: &[f32]) -> String {
    format!(
        "[{}]",
        embedding
            .iter()
            .map(f32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VectorSearchResult {
    pub tool_id: String,
    pub version: String,
    pub score: f32,
}
