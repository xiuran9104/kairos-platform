//! Vision analysis adapter (D-10).
//!
//! Screenshots / verification codes are analyzed by a vision-capable chat
//! model; only the resulting text summary is returned to the agent, so image
//! bytes never pollute the main model context budget. The summary is recorded
//! as evidence through the normal tool-result path.

use kairos_platform_framework_database::PgPool;

/// Resolve the vision model: explicit `PENTA_VISION_MODEL_ID` wins, then the
/// enabled conversation model binding is reused as a fallback.
pub async fn resolve_vision_model(pool: &PgPool) -> Option<i64> {
    if let Ok(raw) = std::env::var("PENTA_VISION_MODEL_ID") {
        if let Ok(id) = raw.trim().parse::<i64>() {
            if id > 0 {
                return Some(id);
            }
        }
    }
    sqlx::query_scalar::<_, i64>(
        "SELECT b.model_config_id FROM pent_model_role_bindings b
         JOIN ai.model_configs m ON m.id=b.model_config_id
         WHERE b.model_role='conversation' AND b.enabled=true
           AND m.type='chat' AND m.status=0 AND m.deleted=false
         ORDER BY (tenant_id=0) DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Analyze an image (data URL) with a text prompt, returning a text summary.
pub async fn analyze_image(
    pool: &PgPool,
    image_data_url: &str,
    prompt: &str,
) -> Result<String, String> {
    let Some(model_id) = resolve_vision_model(pool).await else {
        return Err("未配置视觉模型（PENTA_VISION_MODEL_ID 或会话模型）".into());
    };
    let factory = kairos_platform_ai_server::AiModelFactory::new(pool.clone());
    factory
        .vision(model_id, image_data_url, prompt)
        .await
        .map_err(|error| format!("视觉分析失败: {error:?}"))
}

/// Return a normalized prompt for screenshot triage, appending the caller's
/// question when provided.
pub fn screenshot_prompt(question: Option<&str>) -> String {
    let mut prompt = "你是授权渗透测试的视觉分析助手。请描述截图中的页面内容，重点识别：\
        表单与输入点、登录/鉴权状态、错误信息、验证码/OTP、敏感数据暴露、以及任何可利用的\
        安全线索。只输出文字摘要，不输出图片。"
        .to_string();
    if let Some(question) = question.map(str::trim).filter(|q| !q.is_empty()) {
        prompt.push_str("\n\n用户关注的问题：");
        prompt.push_str(question);
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::screenshot_prompt;

    #[test]
    fn screenshot_prompt_is_a_text_only_summary_request() {
        let prompt = screenshot_prompt(None);
        assert!(prompt.contains("只输出文字摘要"));
    }

    #[test]
    fn screenshot_prompt_appends_the_question() {
        let prompt = screenshot_prompt(Some("识别验证码"));
        assert!(prompt.contains("识别验证码"));
    }
}
