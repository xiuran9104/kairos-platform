//! Embedding provider — generates vector embeddings for knowledge documents.
//!
//! Uses the configured AI model (OpenAI-compatible API) for text embeddings.

use kairos_platform_framework_database::PgPool;
use serde_json::Value;

pub struct EmbeddingProvider {
    pool: PgPool,
    model: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
}

impl EmbeddingProvider {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            model: None,
            base_url: None,
            api_key: None,
        }
    }

    /// Try to load embedding model config from ai.model_configs.
    /// Falls back gracefully — knowledge base still works without embeddings.
    pub async fn try_configure(&mut self) -> Result<bool, String> {
        let row = sqlx::query_as::<_, (String, String, String)>(
            "SELECT model, url, api_key FROM ai.model_configs
             WHERE type = 'embedding' AND status = 0
             ORDER BY id LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        if let Some((model, url, key)) = row {
            self.model = Some(model);
            self.base_url = Some(url);
            self.api_key = Some(key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn is_available(&self) -> bool {
        self.model.is_some() && self.base_url.is_some() && self.api_key.is_some()
    }

    /// Generate embedding vector for a text.
    /// Returns None if no embedding model is configured.
    pub async fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let (model, base_url, api_key) = (
            self.model.as_ref()?,
            self.base_url.as_ref()?,
            self.api_key.as_ref()?,
        );

        let endpoint = format!("{}/embeddings", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "input": truncate_text(text, 8000),
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&endpoint)
            .header(
                "Authorization",
                format!("Bearer {}", api_key.trim_start_matches("Bearer ")),
            )
            .json(&body)
            .send()
            .await
            .ok()?;

        let value: Value = resp.json().await.ok()?;
        value
            .pointer("/data/0/embedding")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_f64())
                    .map(|f| f as f32)
                    .collect()
            })
    }
}

fn truncate_text(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
