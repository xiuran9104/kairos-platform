//! Skill lifecycle management — skills are reviewed operational playbooks,
//! separate from the passive knowledge base. Only `published` skills are
//! exposed to the agent (index via `skill_list`, full text via `skill_load`);
//! skills never widen scope and never produce evidence by themselves.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, put},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

const MAX_SKILL_CONTENT_CHARS: usize = 12_000;
const ALLOWED_TRANSITIONS: &[(&str, &str)] = &[
    ("draft", "reviewed"),
    ("reviewed", "published"),
    ("reviewed", "draft"),
    ("published", "disabled"),
    ("disabled", "draft"),
];

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/skills", get(list_skills).post(create_skill))
        .route("/pent/skills/stats", get(skill_stats).delete(reset_skill_stats))
        .route("/pent/skills/{id}", get(get_skill).put(update_skill))
        .route("/pent/skills/{id}/status", put(update_skill_status))
}

async fn list_skills(
    State(s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let status = params
        .get("status")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let category = params
        .get("category")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let query = params
        .get("query")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(s) FROM (
             SELECT id, name, title, description, category, risk_level, status,
                    version, revision, created_at, updated_at
             FROM pent_skills
             WHERE ($1::text IS NULL OR status=$1)
               AND ($2::text IS NULL OR category=$2)
               AND ($3::text IS NULL OR name ILIKE '%'||$3||'%' OR title ILIKE '%'||$3||'%'
                    OR description ILIKE '%'||$3||'%')
             ORDER BY category, name
         ) s",
    )
    .bind(status)
    .bind(category)
    .bind(query)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn get_skill(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let value =
        sqlx::query_scalar::<_, Value>("SELECT to_jsonb(s) FROM pent_skills s WHERE s.id=$1")
            .bind(&id)
            .fetch_optional(&s.pool)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?
            .ok_or_else(|| AppError::not_found("Skill 不存在"))?;
    Ok(Json(ApiResponse::new(value)))
}

async fn create_skill(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("name 不能为空"))?;
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.len() < 2
        || name.len() > 64
    {
        return Err(AppError::bad_request(
            "name 只能包含小写字母、数字和中划线，长度 2-64",
        ));
    }
    let title = body
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("title 不能为空"))?;
    let content = body
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("content 不能为空"))?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pent_skills(
             id, name, title, description, category, risk_level, status,
             manifest, content, version
         ) VALUES($1,$2,$3,$4,$5,$6,'draft',$7,$8,$9)",
    )
    .bind(&id)
    .bind(name)
    .bind(title)
    .bind(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )
    .bind(
        body.get("category")
            .and_then(Value::as_str)
            .unwrap_or("general"),
    )
    .bind(
        body.get("risk_level")
            .and_then(Value::as_str)
            .unwrap_or("low"),
    )
    .bind(body.get("manifest").cloned().unwrap_or_else(|| json!({})))
    .bind(content)
    .bind(
        body.get("version")
            .and_then(Value::as_str)
            .unwrap_or("1.0.0"),
    )
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(format!("创建 Skill 失败（名称版本重复？）：{error}")))?;
    let created =
        sqlx::query_scalar::<_, Value>("SELECT to_jsonb(s) FROM pent_skills s WHERE s.id=$1")
            .bind(&id)
            .fetch_one(&s.pool)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(created)))
}

async fn update_skill_status(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let target = body
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("status 不能为空"))?;
    let current: Option<String> = sqlx::query_scalar("SELECT status FROM pent_skills WHERE id=$1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let current = current.ok_or_else(|| AppError::not_found("Skill 不存在"))?;
    if current == target {
        return Err(AppError::bad_request("状态未变化"));
    }
    if !is_allowed_transition(&current, target) {
        return Err(AppError::bad_request(format!(
            "不允许的状态流转：{current} → {target}（合法路径：draft→reviewed→published→disabled）"
        )));
    }
    let updated = sqlx::query_scalar::<_, Value>(
        "UPDATE pent_skills SET status=$2, revision=revision+1, updated_at=now()
         WHERE id=$1 RETURNING to_jsonb(pent_skills)",
    )
    .bind(&id)
    .bind(target)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn update_skill(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let current: Option<String> = sqlx::query_scalar("SELECT status FROM pent_skills WHERE id=$1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let current = current.ok_or_else(|| AppError::not_found("Skill 不存在"))?;
    if !matches!(current.as_str(), "draft" | "reviewed") {
        return Err(AppError::bad_request(format!(
            "仅 draft/reviewed 状态的 Skill 可编辑，当前状态：{current}"
        )));
    }
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || name.len() < 2
            || name.len() > 64
        {
            return Err(AppError::bad_request(
                "name 只能包含小写字母、数字和中划线，长度 2-64",
            ));
        }
    }
    let updated = sqlx::query_scalar::<_, Value>(
        "UPDATE pent_skills SET
             name=COALESCE($2, name),
             title=COALESCE($3, title),
             description=COALESCE($4, description),
             category=COALESCE($5, category),
             risk_level=COALESCE($6, risk_level),
             content=COALESCE($7, content),
             revision=revision+1,
             updated_at=now()
         WHERE id=$1 RETURNING to_jsonb(pent_skills)",
    )
    .bind(&id)
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("title").and_then(Value::as_str))
    .bind(body.get("description").and_then(Value::as_str))
    .bind(body.get("category").and_then(Value::as_str))
    .bind(body.get("risk_level").and_then(Value::as_str))
    .bind(body.get("content").and_then(Value::as_str))
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(format!("更新 Skill 失败（名称版本重复？）：{error}")))?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn skill_stats(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let stats = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(bucket ORDER BY (bucket->>'total_calls')::int DESC), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'skill_name', c.params_summary->>'name',
                 'total_calls', count(*),
                 'success_calls', count(*) FILTER (WHERE c.exit_code=0),
                 'failed_calls', count(*) FILTER (WHERE c.completed_at IS NOT NULL AND c.exit_code<>0),
                 'last_call_time', max(c.created_at)
             ) AS bucket
             FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
             WHERE c.tool_name='skill_load' AND r.tenant_id=$1
               AND c.created_at >= COALESCE(
                   (SELECT stats_since FROM pent_skill_stats_meta WHERE tenant_id=$1),
                   '-infinity'
               )
             GROUP BY c.params_summary->>'name'
         ) t",
    )
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let total_skills: i64 = sqlx::query_scalar("SELECT count(*) FROM pent_skills")
        .fetch_one(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let total_calls: i64 = stats
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["total_calls"].as_i64())
                .sum()
        })
        .unwrap_or(0);
    let total_success: i64 = stats
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["success_calls"].as_i64())
                .sum()
        })
        .unwrap_or(0);
    let total_failed: i64 = stats
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["failed_calls"].as_i64())
                .sum()
        })
        .unwrap_or(0);
    Ok(Json(ApiResponse::new(json!({
        "total_skills": total_skills,
        "total_calls": total_calls,
        "total_success": total_success,
        "total_failed": total_failed,
        "stats": stats,
    }))))
}

async fn reset_skill_stats(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let stats_since = sqlx::query_scalar::<_, Value>(
        "INSERT INTO pent_skill_stats_meta(tenant_id, stats_since) VALUES($1, now())
         ON CONFLICT(tenant_id) DO UPDATE SET stats_since=now()
         RETURNING to_jsonb(pent_skill_stats_meta)->'stats_since'",
    )
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"stats_since": stats_since}))))
}

/// Agent-facing: index of published skills (no content bodies).
pub(crate) async fn list_published_skills(
    pool: &kairos_platform_framework_database::PgPool,
    category: Option<&str>,
    query: Option<&str>,
    limit: i64,
) -> Result<Value, String> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, String)>(
        "SELECT name, title, category, risk_level, description, version
         FROM pent_skills
         WHERE status='published'
           AND ($1::text IS NULL OR category=$1)
           AND ($2::text IS NULL OR name ILIKE '%'||$2||'%' OR title ILIKE '%'||$2||'%'
                OR description ILIKE '%'||$2||'%')
         ORDER BY category, name
         LIMIT $3",
    )
    .bind(category)
    .bind(query)
    .bind(limit.clamp(1, 50))
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "items": rows.into_iter().map(|row| json!({
            "name": row.0,
            "title": row.1,
            "category": row.2,
            "risk_level": row.3,
            "description": row.4,
            "version": row.5,
        })).collect::<Vec<_>>(),
        "hint": "使用 skill_load 按 name 读取完整规程；Skill 不扩大授权范围。",
    }))
}

/// Agent-facing: full text of one published skill, bounded.
pub(crate) async fn load_published_skill(
    pool: &kairos_platform_framework_database::PgPool,
    name: &str,
) -> Result<Value, String> {
    let row = sqlx::query_as::<_, (String, String, String, String, Value, String)>(
        "SELECT name, title, category, risk_level, manifest, content
         FROM pent_skills WHERE status='published' AND name=$1
         ORDER BY revision DESC LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("已发布 Skill 中不存在 '{name}'"))?;
    let char_count = row.5.chars().count();
    let truncated = char_count > MAX_SKILL_CONTENT_CHARS;
    let content: String = row.5.chars().take(MAX_SKILL_CONTENT_CHARS).collect();
    Ok(json!({
        "name": row.0,
        "title": row.1,
        "category": row.2,
        "risk_level": row.3,
        "manifest": row.4,
        "content": content,
        "truncated": truncated,
        "total_chars": char_count,
    }))
}

fn is_allowed_transition(from: &str, to: &str) -> bool {
    ALLOWED_TRANSITIONS
        .iter()
        .any(|(allowed_from, allowed_to)| *allowed_from == from && *allowed_to == to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_follows_draft_reviewed_published_path() {
        assert!(is_allowed_transition("draft", "reviewed"));
        assert!(is_allowed_transition("reviewed", "published"));
        assert!(is_allowed_transition("reviewed", "draft"));
        assert!(is_allowed_transition("published", "disabled"));
        assert!(is_allowed_transition("disabled", "draft"));
        assert!(!is_allowed_transition("draft", "published"));
        assert!(!is_allowed_transition("published", "draft"));
        assert!(!is_allowed_transition("disabled", "published"));
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL (TEST_DATABASE_URL)"]
    async fn published_skills_are_listed_and_loadable() {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        let pool = kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database");

        let index = list_published_skills(&pool, None, None, 20)
            .await
            .expect("list published skills");
        let items = index["items"].as_array().expect("items array");
        assert!(
            items.len() >= 4,
            "种子 Skill 必须可见，实际 {} 个",
            items.len()
        );
        assert!(items.iter().all(|item| item.get("content").is_none()));
        let names: Vec<&str> = items
            .iter()
            .filter_map(|item| item["name"].as_str())
            .collect();
        assert!(names.contains(&"web-recon"));
        assert!(names.contains(&"sqli-validation"));

        let filtered = list_published_skills(&pool, Some("recon"), None, 20)
            .await
            .expect("filter by category");
        let filtered_items = filtered["items"].as_array().unwrap();
        assert_eq!(filtered_items.len(), 1);
        assert_eq!(filtered_items[0]["name"], "web-recon");

        let loaded = load_published_skill(&pool, "sqli-validation")
            .await
            .expect("load skill");
        assert_eq!(loaded["title"], "SQL 注入验证");
        assert!(loaded["content"].as_str().unwrap().contains("证据要求"));
        assert_eq!(loaded["truncated"], false);
        assert!(loaded["manifest"]["required_tools"].is_array());

        let missing = load_published_skill(&pool, "no-such-skill").await;
        assert!(missing.is_err(), "未发布或不存在的 Skill 必须报错");

        // Draft skills must not leak into the agent-facing index.
        sqlx::query(
            "INSERT INTO pent_skills(id, name, title, content, status)
             VALUES ('skill-it-draft', 'it-draft-skill', '草稿', '内容', 'draft')",
        )
        .execute(&pool)
        .await
        .expect("seed draft skill");
        let index = list_published_skills(&pool, None, Some("it-draft"), 20)
            .await
            .expect("list after draft insert");
        assert_eq!(index["items"].as_array().unwrap().len(), 0);
        let load = load_published_skill(&pool, "it-draft-skill").await;
        assert!(load.is_err(), "草稿 Skill 不得被加载");
        sqlx::query("DELETE FROM pent_skills WHERE id='skill-it-draft'")
            .execute(&pool)
            .await
            .expect("cleanup draft skill");
    }
}
