use kairos_platform_ai_api::{AiModelType, AiPlatform};
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoverModels {
    pub(crate) config_id: Option<i64>,
    pub(crate) platform: Option<String>,
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) api_key: String,
}

struct DiscoveryConfig {
    platform: AiPlatform,
    url: String,
    api_key: String,
}

pub(crate) async fn discover(pool: &PgPool, request: DiscoverModels) -> Result<Value, AppError> {
    let config = resolve_discovery_config(pool, request).await?;
    let source = format!("{}/models", config.url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let response = if config.platform == AiPlatform::Gemini {
        client
            .get(&source)
            .query(&[("key", config.api_key.as_str())])
            .send()
            .await
    } else {
        let mut builder = client.get(&source);
        if !config.api_key.is_empty() {
            builder = builder.bearer_auth(config.api_key.trim_start_matches("Bearer "));
        }
        builder.send().await
    }
    .map_err(|error| AppError::bad_request(format!("获取模型列表失败：{error}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| AppError::bad_request(format!("读取模型列表失败：{error}")))?;
    let value = serde_json::from_str::<Value>(&body);
    if !status.is_success() {
        let message = value
            .as_ref()
            .ok()
            .and_then(|value| value.pointer("/error/message"))
            .and_then(Value::as_str)
            .or_else(|| (!body.trim().is_empty()).then_some(body.trim()))
            .unwrap_or("供应商拒绝获取模型列表");
        return Err(AppError::bad_request(format!(
            "模型目录同步失败（HTTP {}）：{}",
            status.as_u16(),
            truncate(message, 500)
        )));
    }
    let value = value
        .map_err(|error| AppError::bad_request(format!("模型列表响应不是有效 JSON：{error}")))?;
    let models = parse_models(config.platform, value);
    let synced_at = chrono::Utc::now().timestamp_millis();
    persist_models(pool, config.platform, &source, synced_at, &models).await?;

    Ok(json!({
        "models": models,
        "platform": config.platform.code(),
        "source": source,
        "syncedAt": synced_at,
        "persisted": true,
    }))
}

async fn resolve_discovery_config(
    pool: &PgPool,
    request: DiscoverModels,
) -> Result<DiscoveryConfig, AppError> {
    let (platform, url, mut api_key) = if let Some(config_id) = request.config_id {
        let row = sqlx::query(
            "SELECT platform,url,api_key FROM ai.model_configs WHERE id=$1 AND deleted=false",
        )
        .bind(config_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::internal("failed to load AI model configuration"))?
        .ok_or_else(|| AppError::not_found("AI model not found"))?;
        (
            row.get::<String, _>("platform"),
            row.get::<String, _>("url"),
            row.get::<String, _>("api_key"),
        )
    } else {
        (
            request
                .platform
                .ok_or_else(|| AppError::bad_request("模型平台不能为空"))?,
            request
                .url
                .ok_or_else(|| AppError::bad_request("API 地址不能为空"))?,
            request.api_key,
        )
    };

    let platform = AiPlatform::parse(&platform)
        .ok_or_else(|| AppError::bad_request("unsupported platform"))?;
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(AppError::bad_request("API 地址不能为空"));
    }
    reqwest::Url::parse(&url).map_err(|_| AppError::bad_request("API 地址格式无效"))?;

    if api_key.trim().is_empty() && platform != AiPlatform::Ollama {
        api_key = sqlx::query_scalar::<_, String>(
            "SELECT api_key FROM ai.model_configs
             WHERE platform=$1 AND url=$2 AND api_key<>'' AND deleted=false
             ORDER BY update_time DESC LIMIT 1",
        )
        .bind(platform.code())
        .bind(&url)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::internal("failed to reuse AI model credential"))?
        .unwrap_or_default();
    }

    Ok(DiscoveryConfig {
        platform,
        url,
        api_key,
    })
}

fn parse_models(platform: AiPlatform, value: Value) -> Vec<Value> {
    value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            item.get("id")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .map(|id| id.trim_start_matches("models/").to_string())
        })
        .filter_map(|id| {
            let model_type = AiModelType::parse(discovered_model_type(&id))?;
            platform
                .supports(model_type)
                .then(|| json!({"id":id,"type":model_type.code()}))
        })
        .collect()
}

async fn persist_models(
    pool: &PgPool,
    platform: AiPlatform,
    source: &str,
    synced_at: i64,
    models: &[Value],
) -> Result<(), AppError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to start model catalog sync"))?;
    sqlx::query(
        "UPDATE ai.model_catalog
         SET missing_count=missing_count+1,
             active=CASE WHEN missing_count+1 >= 2 THEN FALSE ELSE active END
         WHERE platform=$1 AND source='discover'",
    )
    .bind(platform.code())
    .execute(&mut *transaction)
    .await
    .map_err(|_| AppError::internal("failed to age old model catalog"))?;
    for model in models {
        sqlx::query(
            "INSERT INTO ai.model_catalog(
                platform,model,type,source,source_url,active,synced_at,missing_count
             ) VALUES($1,$2,$3,'discover',$4,TRUE,$5,0)
             ON CONFLICT(platform,model) DO UPDATE
             SET type=EXCLUDED.type,source='discover',source_url=EXCLUDED.source_url,
                 active=TRUE,synced_at=EXCLUDED.synced_at,missing_count=0",
        )
        .bind(platform.code())
        .bind(model["id"].as_str().unwrap_or_default())
        .bind(model["type"].as_str().unwrap_or("chat"))
        .bind(source)
        .bind(synced_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AppError::internal("failed to persist model catalog"))?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| AppError::internal("failed to commit model catalog sync"))?;
    Ok(())
}

fn discovered_model_type(model: &str) -> &'static str {
    let id = model.to_ascii_lowercase();
    if id.contains("embedding") || id.contains("embed") {
        "embedding"
    } else if id.contains("rerank") {
        "rerank"
    } else if id.contains("transcrib") || id.contains("whisper") {
        "transcription"
    } else if id.contains("tts") || id.contains("audio") {
        "speech"
    } else if id.contains("sora") || id.contains("video") || id.contains("seedance") {
        "video"
    } else if id.contains("image")
        || id.contains("seedream")
        || id.contains("dall-e")
        || id.contains("flux")
        || id.contains("cogview")
    {
        "image"
    } else {
        "chat"
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::{discovered_model_type, truncate};

    #[test]
    fn discovered_models_are_classified_before_merging_with_presets() {
        assert_eq!(discovered_model_type("qwen3-rerank"), "rerank");
        assert_eq!(discovered_model_type("qwen3-vl-rerank"), "rerank");
        assert_eq!(discovered_model_type("text-embedding-v4"), "embedding");
        assert_eq!(
            discovered_model_type("doubao-seedance-1-5-pro-251215"),
            "video"
        );
        assert_eq!(discovered_model_type("doubao-seedream-4-5-251128"), "image");
        assert_eq!(discovered_model_type("qwen-max"), "chat");
    }

    #[test]
    fn provider_error_bodies_are_bounded() {
        assert_eq!(truncate("认证失败", 2), "认证");
    }
}
