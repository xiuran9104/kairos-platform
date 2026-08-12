//! Tool/MCP call monitoring — paginated execution list, aggregate stats,
//! timeline buckets and per-execution drill-down over `pent_tool_calls`.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

const STATUS_CASE: &str =
    "CASE WHEN c.completed_at IS NULL THEN 'running' WHEN c.exit_code=0 THEN 'completed' ELSE 'failed' END";

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/monitor", get(list_executions))
        .route("/pent/monitor/calls-timeline", get(calls_timeline))
        .route(
            "/pent/monitor/execution/{id}",
            get(get_execution).delete(delete_execution),
        )
        .route("/pent/monitor/executions", delete(delete_executions))
}

async fn list_executions(
    State(s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let page = params
        .get("page")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let status = params
        .get("status")
        .map(String::as_str)
        .filter(|v| matches!(*v, "running" | "completed" | "failed"));
    let tool = params
        .get("tool")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let offset = (page - 1) * page_size;

    let executions = sqlx::query_scalar::<_, Value>(&format!(
        "SELECT jsonb_build_object(
             'id', c.id, 'run_id', c.run_id, 'tool_name', c.tool_name,
             'status', {STATUS_CASE},
             'started_at', c.started_at, 'completed_at', c.completed_at,
             'duration_ms', c.duration_ms, 'exit_code', c.exit_code,
             'executor_backend', c.executor_backend, 'params_summary', c.params_summary
         )
         FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
         WHERE r.tenant_id=$1
           AND ($2::text IS NULL OR {STATUS_CASE}=$2)
           AND ($3::text IS NULL OR c.tool_name ILIKE '%'||$3||'%')
         ORDER BY c.created_at DESC LIMIT $4 OFFSET $5"
    ))
    .bind(tenant_id)
    .bind(status)
    .bind(tool)
    .bind(page_size)
    .bind(offset)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
         WHERE r.tenant_id=$1
           AND ($2::text IS NULL OR {STATUS_CASE}=$2)
           AND ($3::text IS NULL OR c.tool_name ILIKE '%'||$3||'%')"
    ))
    .bind(tenant_id)
    .bind(status)
    .bind(tool)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    // Tenant-wide aggregates — not affected by pagination or status/tool filters.
    let summary = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'total_calls', count(*),
             'success_calls', count(*) FILTER (WHERE c.exit_code=0),
             'failed_calls', count(*) FILTER (WHERE c.completed_at IS NOT NULL AND c.exit_code<>0),
             'last_call_time', max(c.created_at),
             'tool_count', count(DISTINCT c.tool_name)
         )
         FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
         WHERE r.tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let top_tools = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(bucket ORDER BY (bucket->>'calls')::int DESC), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'tool_name', c.tool_name,
                 'calls', count(*),
                 'successes', count(*) FILTER (WHERE c.exit_code=0),
                 'failures', count(*) FILTER (WHERE c.completed_at IS NOT NULL AND c.exit_code<>0)
             ) AS bucket
             FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
             WHERE r.tenant_id=$1
             GROUP BY c.tool_name
             ORDER BY count(*) DESC
             LIMIT 10
         ) t",
    )
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let total_pages = if total == 0 {
        0
    } else {
        (total + page_size - 1) / page_size
    };
    Ok(Json(ApiResponse::new(json!({
        "executions": executions,
        "summary": summary,
        "top_tools": top_tools,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages,
    }))))
}

async fn calls_timeline(
    State(s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let range = params
        .get("range")
        .map(String::as_str)
        .unwrap_or("24h");
    let (trunc, interval) = match range {
        "7d" => ("day", "7 days"),
        "30d" => ("day", "30 days"),
        _ => ("hour", "24 hours"),
    };
    let buckets = sqlx::query_scalar::<_, Value>(&format!(
        "SELECT COALESCE(jsonb_agg(bucket ORDER BY bucket->>'ts'), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'ts', date_trunc('{trunc}', c.created_at),
                 'total', count(*),
                 'success', count(*) FILTER (WHERE c.exit_code=0),
                 'failed', count(*) FILTER (WHERE c.completed_at IS NOT NULL AND c.exit_code<>0)
             ) AS bucket
             FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
             WHERE r.tenant_id=$1 AND c.created_at >= now() - interval '{interval}'
             GROUP BY date_trunc('{trunc}', c.created_at)
         ) t"
    ))
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"buckets": buckets}))))
}

async fn get_execution(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let execution = sqlx::query_scalar::<_, Value>(&format!(
        "SELECT jsonb_build_object(
             'id', c.id, 'run_id', c.run_id, 'tool_name', c.tool_name,
             'status', {STATUS_CASE},
             'started_at', c.started_at, 'completed_at', c.completed_at,
             'duration_ms', c.duration_ms, 'exit_code', c.exit_code,
             'executor_backend', c.executor_backend, 'params_summary', c.params_summary,
             'execution_audit', c.execution_audit
         )
         FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
         WHERE c.id=$1 AND r.tenant_id=$2"
    ))
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("执行记录不存在"))?;
    let evidence = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(item ORDER BY item->>'id'), '[]'::jsonb)
         FROM (
             SELECT jsonb_build_object(
                 'id', e.id, 'content_type', e.content_type, 'source', left(e.source, 20000)
             ) AS item
             FROM pent_evidence e
             WHERE e.tool_call_id=$1
         ) t",
    )
    .bind(&id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({
        "execution": execution,
        "evidence": evidence,
    }))))
}

async fn delete_execution(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query("DELETE FROM pent_evidence WHERE tool_call_id=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let affected = sqlx::query(
        "DELETE FROM pent_tool_calls c USING pent_runs r
         WHERE c.run_id=r.id AND c.id=$1 AND r.tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("执行记录不存在"));
    }
    tx.commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn delete_executions(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let ids: Vec<String> = body
        .get("ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(AppError::bad_request("ids 不能为空"));
    }
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query("DELETE FROM pent_evidence WHERE tool_call_id=ANY($1)")
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let deleted = sqlx::query(
        "DELETE FROM pent_tool_calls c USING pent_runs r
         WHERE c.run_id=r.id AND c.id=ANY($1) AND r.tenant_id=$2",
    )
    .bind(&ids)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    tx.commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"deleted": deleted}))))
}
