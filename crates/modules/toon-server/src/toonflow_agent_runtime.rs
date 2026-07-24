use sqlx::PgPool;
use std::collections::HashSet;

async fn embedding(pool: &PgPool, text: &str) -> Option<Vec<f32>> {
    let model_id: i64 = sqlx::query_scalar(
        "SELECT id FROM ai.model_configs WHERE type='embedding' AND status=0 ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()??;
    kairos_platform_ai_server::AiModelFactory::new(pool.clone())
        .embedding(
            model_id,
            kairos_platform_ai_api::EmbeddingRequest {
                inputs: vec![text.to_string()],
            },
        )
        .await
        .ok()?
        .embeddings
        .into_iter()
        .next()
}

pub(crate) async fn store_memory_embedding(pool: &PgPool, id: i64, content: &str) {
    let Some(vector) = embedding(pool, content).await else {
        return;
    };
    let _ = sqlx::query("UPDATE toonflow.agent_memories SET embedding=$2 WHERE id=$1")
        .bind(id)
        .bind(serde_json::json!(vector))
        .execute(pool)
        .await;
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let dot = left.iter().zip(right).map(|(a, b)| a * b).sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

pub(crate) fn skill_path(agent_key: &str) -> Option<&'static str> {
    match agent_key {
        "scriptAgent:decisionAgent" => Some("script_agent_decision.md"),
        "scriptAgent:supervisionAgent" => Some("script_agent_supervision.md"),
        "scriptAgent:storySkeletonAgent" => Some("script_execution_skeleton.md"),
        "scriptAgent:adaptationStrategyAgent" => Some("script_execution_adaptation.md"),
        "scriptAgent:scriptAgent" => Some("script_execution_script.md"),
        "productionAgent:decisionAgent" => Some("production_agent_decision.md"),
        "productionAgent:supervisionAgent" => Some("production_agent_supervision.md"),
        "productionAgent:deriveAssetsAgent" => Some("production_execution_derive_assets.md"),
        "productionAgent:generateAssetsAgent" => Some("production_execution_generate_assets.md"),
        "productionAgent:directorPlanAgent" => Some("production_execution_director_plan.md"),
        "productionAgent:storyboardGenAgent" => Some("production_execution_storyboard_gen.md"),
        "productionAgent:storyboardPanelAgent" => Some("production_execution_storyboard_panel.md"),
        "productionAgent:storyboardTableAgent" => Some("production_execution_storyboard_table.md"),
        _ => None,
    }
}

pub(crate) async fn load_agent_skill(pool: &PgPool, agent_key: &str) -> Result<String, String> {
    let path = skill_path(agent_key).ok_or_else(|| format!("Agent {agent_key} 没有对应 Skill"))?;
    load_skill(pool, path).await
}

pub(crate) async fn load_skill(pool: &PgPool, path: &str) -> Result<String, String> {
    sqlx::query_scalar(
        "SELECT content FROM toonflow.skill_list WHERE path=$1 AND state=1 AND content<>''",
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Skill {path} 不存在或已停用"))
}

pub(crate) async fn dynamic_skills(pool: &PgPool) -> Result<Vec<(String, String)>, String> {
    sqlx::query_as(
        "SELECT path,name FROM toonflow.skill_list WHERE state=1 AND (path LIKE 'production_skills/%' OR path LIKE 'art_skills/%' OR path LIKE 'story_skills/%') ORDER BY path",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

fn terms(value: &str) -> HashSet<String> {
    let lower = value.to_lowercase();
    let chars = lower.chars().collect::<Vec<_>>();
    let mut result = lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_string)
        .collect::<HashSet<_>>();
    for pair in chars.windows(2) {
        if pair.iter().all(|character| !character.is_whitespace()) {
            result.insert(pair.iter().collect());
        }
    }
    result
}

pub(crate) async fn relevant_memories(
    pool: &PgPool,
    agent_type: &str,
    isolation_key: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let rows: Vec<(i64, String, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT create_time,content,embedding FROM toonflow.agent_memories WHERE agent_type=$1 AND isolation_key=$2 AND memory_type='message' ORDER BY create_time DESC LIMIT 100",
    )
    .bind(agent_type)
    .bind(isolation_key)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let query_embedding = embedding(pool, query).await;
    let query_terms = terms(query);
    let mut ranked = rows
        .into_iter()
        .filter_map(|(time, content, stored)| {
            let semantic = query_embedding.as_ref().and_then(|query_vector| {
                let vector = serde_json::from_value::<Vec<f32>>(stored?).ok()?;
                Some(cosine(query_vector, &vector))
            });
            let lexical = terms(&content).intersection(&query_terms).count() as f32;
            let score = semantic.map(|value| value * 1000.0).unwrap_or(lexical);
            (score > 0.0).then_some((score, time, content))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0).then(right.1.cmp(&left.1)));
    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, content)| content)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::skill_path;

    #[test]
    fn maps_every_agent_to_the_imported_skill() {
        assert_eq!(
            skill_path("scriptAgent:storySkeletonAgent"),
            Some("script_execution_skeleton.md")
        );
        assert_eq!(
            skill_path("productionAgent:storyboardTableAgent"),
            Some("production_execution_storyboard_table.md")
        );
    }
}
