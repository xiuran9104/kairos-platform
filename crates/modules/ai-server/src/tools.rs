use crate::{AiState, require};
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{delete, get, post, put},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

pub(crate) async fn load_definitions(
    pool: &sqlx::PgPool,
    ids: &[i64],
) -> Result<Vec<Value>, AppError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let rows = sqlx::query(
        "SELECT name,description,input_schema FROM ai.tools WHERE id=ANY($1) AND status=1",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("读取对话工具失败"))?;
    Ok(rows.into_iter().map(|r|json!({"type":"function","function":{"name":r.get::<String,_>("name"),"description":r.get::<String,_>("description"),"parameters":r.get::<Value,_>("input_schema")}})).collect())
}
pub(crate) async fn execute(
    pool: &sqlx::PgPool,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    let allowed: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ai.tools WHERE name=$1 AND status=1)")
            .bind(name)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
    if !allowed {
        return Err(format!("工具 {name} 未启用"));
    }
    match name {
        "current_time" => {
            let offset = args
                .get("utcOffset")
                .and_then(Value::as_str)
                .ok_or("utcOffset 不能为空")?;
            let seconds = parse_offset(offset)?;
            let zone = chrono::FixedOffset::east_opt(seconds).ok_or("UTC 偏移无效")?;
            Ok(chrono::Utc::now().with_timezone(&zone).to_rfc3339())
        }
        "weather_query" => {
            let location = args
                .get("location")
                .and_then(Value::as_str)
                .ok_or("location 不能为空")?;
            let response = reqwest::Client::new()
                .get("https://wttr.in/")
                .query(&[("format", "j1"), ("q", location)])
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("天气服务返回 {}", response.status()));
            }
            response.text().await.map_err(|e| e.to_string())
        }
        _ => Err(format!("工具 {name} 没有已注册的 Rust 执行器")),
    }
}
fn parse_offset(value: &str) -> Result<i32, String> {
    let sign = if value.starts_with('-') { -1 } else { 1 };
    let clean = value.trim_start_matches(['+', '-']);
    let (h, m) = clean.split_once(':').ok_or("UTC 偏移格式应为 +08:00")?;
    let hours: i32 = h.parse().map_err(|_| "UTC 小时无效")?;
    let minutes: i32 = m.parse().map_err(|_| "UTC 分钟无效")?;
    if hours > 23 || minutes > 59 {
        return Err("UTC 偏移超出范围".into());
    }
    Ok(sign * (hours * 3600 + minutes * 60))
}

#[cfg(test)]
mod tests {
    use super::parse_offset;
    #[test]
    fn parses_utc_offsets() {
        assert_eq!(parse_offset("+08:00").unwrap(), 28_800);
        assert_eq!(parse_offset("-05:30").unwrap(), -19_800);
        assert!(parse_offset("25:00").is_err());
    }
}

pub fn routes() -> Router<AiState> {
    Router::new()
        .route("/ai/tool/page", get(page))
        .route("/ai/tool/simple-list", get(simple_list))
        .route("/ai/tool/get", get(get_one))
        .route("/ai/tool/create", post(create))
        .route("/ai/tool/update", put(update))
        .route("/ai/tool/delete", delete(remove))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageQuery {
    page_no: Option<i64>,
    page_size: Option<i64>,
    name: Option<String>,
    status: Option<i32>,
}
#[derive(Deserialize)]
struct Id {
    id: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Save {
    id: Option<i64>,
    name: String,
    description: Option<String>,
    status: Option<i32>,
    input_schema: Option<Value>,
    executor: Option<Value>,
}
fn value(r: sqlx::postgres::PgRow) -> Value {
    json!({"id":r.get::<i64,_>("id"),"name":r.get::<String,_>("name"),"description":r.get::<String,_>("description"),"status":r.get::<i32,_>("status"),"inputSchema":r.get::<Value,_>("input_schema"),"executor":r.get::<Value,_>("executor"),"createTime":r.get::<i64,_>("create_time")})
}
async fn page(
    u: CurrentUser,
    State(s): State<AiState>,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&u, "ai:tool:query")?;
    let p = q.page_no.unwrap_or(1).max(1);
    let n = q.page_size.unwrap_or(10).clamp(1, 200);
    let rows=sqlx::query("SELECT * FROM ai.tools WHERE ($1::text IS NULL OR name ILIKE '%'||$1||'%') AND ($2::int IS NULL OR status=$2) ORDER BY id DESC LIMIT $3 OFFSET $4").bind(&q.name).bind(q.status).bind(n).bind((p-1)*n).fetch_all(&s.pool).await.map_err(|_|AppError::internal("读取工具失败"))?;
    let total:i64=sqlx::query_scalar("SELECT count(*) FROM ai.tools WHERE ($1::text IS NULL OR name ILIKE '%'||$1||'%') AND ($2::int IS NULL OR status=$2)").bind(&q.name).bind(q.status).fetch_one(&s.pool).await.map_err(|_|AppError::internal("读取工具失败"))?;
    Ok(Json(ApiResponse::new(
        json!({"list":rows.into_iter().map(value).collect::<Vec<_>>(),"total":total}),
    )))
}
async fn simple_list(
    u: CurrentUser,
    State(s): State<AiState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&u, "ai:tool:query")?;
    let rows = sqlx::query("SELECT * FROM ai.tools WHERE status=1 ORDER BY name")
        .fetch_all(&s.pool)
        .await
        .map_err(|_| AppError::internal("读取工具失败"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter().map(value).collect(),
    )))
}
async fn get_one(
    u: CurrentUser,
    State(s): State<AiState>,
    Query(q): Query<Id>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&u, "ai:tool:query")?;
    let r = sqlx::query("SELECT * FROM ai.tools WHERE id=$1")
        .bind(q.id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|_| AppError::internal("读取工具失败"))?
        .ok_or_else(|| AppError::not_found("工具不存在"))?;
    Ok(Json(ApiResponse::new(value(r))))
}
fn validate(v: &Save) -> Result<(), AppError> {
    if v.name.is_empty()
        || !v
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Err(AppError::bad_request("工具名只能包含字母、数字和下划线"))
    } else {
        Ok(())
    }
}
async fn create(
    u: CurrentUser,
    State(s): State<AiState>,
    Json(v): Json<Save>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    require(&u, "ai:tool:create")?;
    validate(&v)?;
    let id = chrono::Utc::now().timestamp_micros();
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO ai.tools(id,name,description,status,input_schema,executor,create_time,update_time)VALUES($1,$2,$3,$4,$5,$6,$7,$7)").bind(id).bind(v.name).bind(v.description.unwrap_or_default()).bind(v.status.unwrap_or(1)).bind(v.input_schema.unwrap_or_else(||json!({"type":"object","properties":{}}))).bind(v.executor.unwrap_or_else(||json!({"kind":"builtin"}))).bind(now).execute(&s.pool).await.map_err(|e|AppError::bad_request(format!("创建工具失败: {e}")))?;
    Ok(Json(ApiResponse::new(id)))
}
async fn update(
    u: CurrentUser,
    State(s): State<AiState>,
    Json(v): Json<Save>,
) -> Result<Json<ApiResponse<bool>>, AppError> {
    require(&u, "ai:tool:update")?;
    validate(&v)?;
    let id =
        v.id.ok_or_else(|| AppError::bad_request("id is required"))?;
    let r=sqlx::query("UPDATE ai.tools SET name=$2,description=$3,status=$4,input_schema=COALESCE($5,input_schema),executor=COALESCE($6,executor),update_time=$7 WHERE id=$1").bind(id).bind(v.name).bind(v.description.unwrap_or_default()).bind(v.status.unwrap_or(1)).bind(v.input_schema).bind(v.executor).bind(chrono::Utc::now().timestamp_millis()).execute(&s.pool).await.map_err(|_|AppError::internal("更新工具失败"))?;
    Ok(Json(ApiResponse::new(r.rows_affected() > 0)))
}
async fn remove(
    u: CurrentUser,
    State(s): State<AiState>,
    Query(q): Query<Id>,
) -> Result<Json<ApiResponse<bool>>, AppError> {
    require(&u, "ai:tool:delete")?;
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ai.chat_conversations WHERE $1=ANY(tool_ids))",
    )
    .bind(q.id)
    .fetch_one(&s.pool)
    .await
    .map_err(|_| AppError::internal("检查工具引用失败"))?;
    if used {
        return Err(AppError::bad_request("工具正在被对话使用"));
    }
    let r = sqlx::query("DELETE FROM ai.tools WHERE id=$1")
        .bind(q.id)
        .execute(&s.pool)
        .await
        .map_err(|_| AppError::internal("删除工具失败"))?;
    Ok(Json(ApiResponse::new(r.rows_affected() > 0)))
}
