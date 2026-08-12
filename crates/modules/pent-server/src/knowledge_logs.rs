//! Knowledge retrieval history — audit log of agent `knowledge_search` calls.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/knowledge/retrieval-logs", get(list_logs))
        .route("/pent/knowledge/retrieval-logs/{id}", axum::routing::delete(delete_log))
}

async fn list_logs(
    State(s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(1, 500);
    let conversation_id = params
        .get("conversation_id")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let message_id = params
        .get("message_id")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(l) FROM pent_knowledge_retrieval_logs l
         WHERE l.tenant_id=$1
           AND ($2::text IS NULL OR l.conversation_id=$2)
           AND ($3::text IS NULL OR l.message_id=$3)
         ORDER BY l.created_at DESC LIMIT $4",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(message_id)
    .bind(limit)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn delete_log(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "DELETE FROM pent_knowledge_retrieval_logs WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("检索记录不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}
