//! AI Model — reads from kairos-platform ai-server model_configs table.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModel {
    pub id: i64,
    pub name: String,
    pub model_name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub max_tokens: usize,
    pub enabled: bool,
}

impl AiModel {
    pub fn default_model() -> Self {
        Self {
            id: 0,
            name: "GPT-4o-mini".into(),
            model_name: "gpt-4o-mini".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
            max_tokens: 128000,
            enabled: true,
        }
    }
}
