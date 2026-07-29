//! Multi-backend execution for Toon tools.
//! Builtin: AI model calls via AiModelFactory.

use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_toon_api::tooling::{
    ToonToolDescriptor, ToonToolExecutionResult, ToonToolInvocation, ToonToolSource,
};

/// Unified execution backend for Toon tools.
#[async_trait::async_trait]
pub trait ToonExecutor: Send + Sync {
    async fn execute(
        &self,
        descriptor: &ToonToolDescriptor,
        invocation: &ToonToolInvocation,
    ) -> anyhow::Result<ToonToolExecutionResult>;
}

/// Builtin executor: routes to AiModelFactory for AI operations.
pub struct BuiltinExecutor {
    pool: PgPool,
}

impl BuiltinExecutor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ToonExecutor for BuiltinExecutor {
    async fn execute(
        &self,
        descriptor: &ToonToolDescriptor,
        invocation: &ToonToolInvocation,
    ) -> anyhow::Result<ToonToolExecutionResult> {
        let start = std::time::Instant::now();
        let factory = AiModelFactory::new(self.pool.clone());
        let handler = descriptor.executor.handler.as_deref().unwrap_or("chat");

        let content = match handler {
            "image" => {
                let request = kairos_platform_ai_api::ImageRequest {
                    prompt: invocation.arguments["prompt"].as_str().unwrap_or("").into(),
                    size: invocation.arguments["size"].as_str().unwrap_or("1024x1024").into(),
                    references: invocation.arguments["references"].as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    seed: invocation.arguments["seed"].as_i64(),
                };
                let model_id = resolve_model(&self.pool, invocation.arguments["model"].as_str().unwrap_or("default")).await?;
                factory.image(model_id, request).await
                    .map(|r| serde_json::to_value(r).unwrap_or_default())
                    .map_err(|e| anyhow::anyhow!("image: {e:?}"))
            }
            _ => {
                // chat or default: call chat_tools
                let system = invocation.arguments["system"].as_str().unwrap_or("");
                let prompt = invocation.arguments["prompt"].as_str().unwrap_or("");
                let messages = vec![
                    serde_json::json!({"role": "system", "content": system}),
                    serde_json::json!({"role": "user", "content": prompt}),
                ];
                let model_id = resolve_model(&self.pool, invocation.arguments["model"].as_str().unwrap_or("default")).await?;
                factory.chat_tools(model_id, messages, vec![], None, None).await
                    .map_err(|e| anyhow::anyhow!("chat: {e:?}"))
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        match content {
            Ok(value) => Ok(ToonToolExecutionResult {
                invocation_id: invocation.invocation_id.clone(),
                tool_id: descriptor.tool_id.clone(),
                tool_version: descriptor.version.clone(),
                success: true,
                exit_code: Some(0),
                content: value,
                content_type: "application/json".into(),
                duration_ms,
                error_message: None,
            }),
            Err(e) => Ok(ToonToolExecutionResult {
                invocation_id: invocation.invocation_id.clone(),
                success: false,
                exit_code: Some(1),
                content: serde_json::json!({"error": e.to_string()}),
                content_type: "application/json".into(),
                duration_ms,
                error_message: Some(e.to_string()),
                tool_id: descriptor.tool_id.clone(),
                tool_version: descriptor.version.clone(),
            }),
        }
    }
}

async fn resolve_model(pool: &PgPool, configured: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "SELECT d.model_config_id FROM toonflow.agent_deployments d
         WHERE d.key = $1
         UNION ALL
         SELECT id FROM ai.model_configs WHERE type = 'chat' AND status = 0 ORDER BY id LIMIT 1",
    )
    .bind(configured)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("no model configured for '{configured}'"))?;
    Ok(id)
}

/// Dispatch to the appropriate executor based on tool source.
pub fn dispatch(pool: PgPool, _descriptor: &ToonToolDescriptor) -> Box<dyn ToonExecutor> {
    Box::new(BuiltinExecutor::new(pool))
}
