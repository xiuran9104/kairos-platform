use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_runtime::ports::{
    KnowledgeGuide, RuntimeEnrichment, RuntimeKnowledgeBlock, RuntimeKnowledgeLayer,
};

#[derive(Clone)]
pub struct PgRuntimeEnrichment {
    pool: PgPool,
}

impl PgRuntimeEnrichment {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RuntimeEnrichment for PgRuntimeEnrichment {
    async fn search_context_knowledge(
        &self,
        query: &str,
        layers: &[RuntimeKnowledgeLayer],
        limit: usize,
    ) -> anyhow::Result<Vec<RuntimeKnowledgeBlock>> {
        Ok(
            super::rmos::knowledge_cache::search_wiki(&self.pool, query, layers, limit)
                .await
                .into_iter()
                .map(|result| RuntimeKnowledgeBlock {
                    title: result.title,
                    layer: result.layer,
                    content: result.content,
                })
                .collect(),
        )
    }

    async fn search_memories(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<String>> {
        let mut embedder =
            crate::infrastructure::embedding::EmbeddingProvider::new(self.pool.clone());
        if !embedder.try_configure().await.unwrap_or(false) {
            return Ok(Vec::new());
        }
        let Some(embedding) = embedder.embed(query).await else {
            return Ok(Vec::new());
        };
        let index = crate::infrastructure::vector::PgVectorIndex::new(self.pool.clone());
        let results = index
            .search(&embedding, None, top_k)
            .await
            .unwrap_or_default();
        Ok(results
            .into_iter()
            .map(|result| {
                format!(
                    "{} v{} (score: {:.2})",
                    result.tool_id, result.version, result.score
                )
            })
            .collect())
    }

    async fn analyze_image(
        &self,
        image_data_url: &str,
        question: Option<&str>,
    ) -> anyhow::Result<String> {
        let prompt = crate::infrastructure::vision::screenshot_prompt(question);
        crate::infrastructure::vision::analyze_image(&self.pool, image_data_url, &prompt)
            .await
            .map_err(anyhow::Error::msg)
    }

    async fn log_knowledge_retrieval(
        &self,
        query: &str,
        guides: &[KnowledgeGuide],
        session_id: &str,
        run_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let retrieved_items: Vec<serde_json::Value> = guides
            .iter()
            .map(|guide| {
                serde_json::json!({
                    "id": if guide.id.is_empty() { guide.title.as_str() } else { guide.id.as_str() },
                    "title": guide.title,
                })
            })
            .collect();
        let tenant_id: i64 = sqlx::query_scalar(
            "SELECT COALESCE(
                 (SELECT tenant_id FROM pent_runs WHERE id=$1),
                 (SELECT tenant_id FROM pent_agent_sessions WHERE id=$2),
                 0
             )",
        )
        .bind(run_id)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO pent_knowledge_retrieval_logs(
                 id, tenant_id, query, risk_type, conversation_id, message_id, retrieved_items
             ) VALUES($1,$2,$3,'',$4,$5,$6)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(tenant_id)
        .bind(query)
        .bind(session_id)
        .bind(Option::<String>::None)
        .bind(serde_json::json!(retrieved_items))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
