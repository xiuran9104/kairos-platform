use serde_json::Value;
use sqlx::PgPool;

async fn agent_model(pool: &PgPool, key: &str) -> Result<(i64, i32, i32), String> {
    let row:Option<(Option<i64>,i32,i32,bool)>=sqlx::query_as("SELECT model_config_id,temperature,max_output_tokens,disabled FROM toonflow.agent_deployments WHERE key=$1").bind(key).fetch_optional(pool).await.map_err(|e|e.to_string())?;
    let (model, temperature, tokens, disabled) =
        row.ok_or_else(|| format!("Agent {key} 未配置"))?;
    if disabled {
        return Err(format!("Agent {key} 已停用"));
    }
    Ok((
        model.ok_or_else(|| format!("Agent {key} 尚未绑定统一 AI 模型"))?,
        temperature,
        tokens,
    ))
}
pub async fn text_tools(
    pool: &PgPool,
    key: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
) -> Result<Value, String> {
    let (model, temperature, tokens) = agent_model(pool, key).await?;
    kairos_platform_ai_server::AiModelFactory::new(pool.clone())
        .chat_tools(
            model,
            messages,
            tools,
            Some(temperature as f64),
            (tokens > 0).then_some(tokens as u32),
        )
        .await
        .map_err(|error| format!("{error:?}"))
}
fn chat_request(
    system: &str,
    user: &str,
    temperature: i32,
    tokens: i32,
) -> kairos_platform_ai_api::ChatRequest {
    kairos_platform_ai_api::ChatRequest {
        model: String::new(),
        messages: vec![
            kairos_platform_ai_api::ChatMessage {
                role: "system".into(),
                content: system.into(),
            },
            kairos_platform_ai_api::ChatMessage {
                role: "user".into(),
                content: user.into(),
            },
        ],
        temperature: Some(temperature as f64),
        max_tokens: (tokens > 0).then_some(tokens as u32),
    }
}
pub async fn text(pool: &PgPool, key: &str, system: &str, user: &str) -> Result<String, String> {
    let (model, temperature, tokens) = agent_model(pool, key).await?;
    kairos_platform_ai_server::AiModelFactory::new(pool.clone())
        .chat(model, chat_request(system, user, temperature, tokens))
        .await
        .map(|x| x.content)
        .map_err(|e| format!("{e:?}"))
}
pub async fn text_stream<F, Fut>(
    pool: &PgPool,
    key: &str,
    system: &str,
    user: &str,
    on_delta: F,
) -> Result<String, String>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let (model, temperature, tokens) = agent_model(pool, key).await?;
    kairos_platform_ai_server::AiModelFactory::new(pool.clone())
        .chat_stream(
            model,
            chat_request(system, user, temperature, tokens),
            on_delta,
        )
        .await
        .map(|x| x.content)
        .map_err(|e| format!("{e:?}"))
}
fn model_id(value: &str, kind: &str) -> Result<i64, String> {
    value
        .parse()
        .map_err(|_| format!("{kind}模型必须是统一 AI 模型 ID"))
}
pub async fn image(
    pool: &PgPool,
    configured: &str,
    prompt: &str,
    size: &str,
) -> Result<String, String> {
    image_with_references(pool, configured, prompt, size, Vec::new(), None).await
}

/// Generates an image with ordered visual references when the configured provider supports edits.
pub async fn image_with_references(
    pool: &PgPool,
    configured: &str,
    prompt: &str,
    size: &str,
    references: Vec<String>,
    seed: Option<i64>,
) -> Result<String, String> {
    kairos_platform_ai_server::AiModelFactory::new(pool.clone())
        .image(
            model_id(configured, "图片")?,
            kairos_platform_ai_api::ImageRequest {
                prompt: prompt.into(),
                size: size.into(),
                references,
                seed,
            },
        )
        .await
        .map(|x| x.url)
        .map_err(|e| format!("{e:?}"))
}
pub async fn video(pool: &PgPool, configured: &str, payload: Value) -> Result<String, String> {
    let model_id = model_id(configured, "视频")?;
    let factory = kairos_platform_ai_server::AiModelFactory::new(pool.clone());
    let response = factory
        .video(model_id, payload)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if !response.url.is_empty() {
        return Ok(response.url);
    }
    let task_id = response
        .task_id
        .ok_or_else(|| "视频响应缺少 URL 或任务 ID".to_string())?;
    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let result = factory
            .poll_video(model_id, &task_id)
            .await
            .map_err(|e| format!("{e:?}"))?;
        if !result.url.is_empty() {
            return Ok(result.url);
        }
    }
    Err(format!("视频任务 {task_id} 等待超时"))
}
pub async fn speech(
    pool: &PgPool,
    configured: &str,
    input: &str,
    voice: &str,
) -> Result<String, String> {
    kairos_platform_ai_server::AiModelFactory::new(pool.clone())
        .speech(
            model_id(configured, "TTS")?,
            kairos_platform_ai_api::SpeechRequest {
                input: input.into(),
                voice: voice.into(),
                format: "mp3".into(),
            },
        )
        .await
        .map(|x| x.url)
        .map_err(|e| format!("{e:?}"))
}
