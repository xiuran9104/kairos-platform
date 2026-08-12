//! HTTP administration surface for the HITL / AI fence.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use kairos_platform_framework_common::response::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{PentState, security};

pub fn routes() -> Router<PentState> {
    Router::new()
        .route(
            "/pent/hitl/settings",
            get(get_settings).put(update_settings),
        )
        .route("/pent/hitl/approvals", get(list_approvals))
        .route("/pent/hitl/approvals/{id}", get(get_approval))
        .route("/pent/hitl/approvals/{id}/decision", post(decide_approval))
}

async fn get_settings(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    let value = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(p) || jsonb_build_object('inherited',p.tenant_id<>$1) FROM pent_runtime_policies p WHERE tenant_id IN ($1,0) ORDER BY (tenant_id=$1) DESC LIMIT 1")
        .bind(tenant).fetch_optional(&s.pool).await.map_err(internal)?
        .unwrap_or_else(|| json!({"hitl_enabled":false,"hitl_mode":"approval","hitl_reviewer":"human","hitl_tool_whitelist":[],"approval_ttl_seconds":1800,"inherited":true}));
    Ok(Json(ApiResponse::new(value)))
}

async fn update_settings(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    let enabled = body["hitl_enabled"].as_bool().unwrap_or(false);
    let mode = body["hitl_mode"].as_str().unwrap_or("approval");
    let reviewer = body["hitl_reviewer"].as_str().unwrap_or("human");
    let whitelist = body
        .get("hitl_tool_whitelist")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let ttl = body["approval_ttl_seconds"].as_i64().unwrap_or(1800);
    if !matches!(mode, "approval" | "review_edit")
        || !matches!(reviewer, "human" | "audit_agent")
        || !whitelist.is_array()
        || !(60..=86400).contains(&ttl)
    {
        return Err(AppError::bad_request("AI 围栏配置无效"));
    }
    let audit_model = body["hitl_audit_model_id"].as_i64();
    let system_prompt = body["hitl_audit_system_prompt"].as_str().unwrap_or("");
    let user_prompt = body["hitl_audit_user_prompt"].as_str().unwrap_or("");
    sqlx::query("INSERT INTO pent_runtime_policies(tenant_id,hitl_enabled,hitl_mode,hitl_reviewer,hitl_tool_whitelist,hitl_audit_model_id,hitl_audit_system_prompt,hitl_audit_user_prompt,approval_ttl_seconds,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(tenant_id) DO UPDATE SET hitl_enabled=EXCLUDED.hitl_enabled,hitl_mode=EXCLUDED.hitl_mode,hitl_reviewer=EXCLUDED.hitl_reviewer,hitl_tool_whitelist=EXCLUDED.hitl_tool_whitelist,hitl_audit_model_id=EXCLUDED.hitl_audit_model_id,hitl_audit_system_prompt=EXCLUDED.hitl_audit_system_prompt,hitl_audit_user_prompt=EXCLUDED.hitl_audit_user_prompt,approval_ttl_seconds=EXCLUDED.approval_ttl_seconds,updated_by=EXCLUDED.updated_by,updated_at=now()")
        .bind(tenant).bind(enabled).bind(mode).bind(reviewer).bind(whitelist).bind(audit_model).bind(system_prompt).bind(user_prompt).bind(ttl as i32).bind(&user.user_id)
        .execute(&s.pool).await.map_err(internal)?;
    Ok(Json(ApiResponse::new(
        json!({"message":"AI 围栏设置已保存"}),
    )))
}

#[derive(Deserialize)]
struct ApprovalQuery {
    status: Option<String>,
    limit: Option<i64>,
}

async fn list_approvals(
    State(s): State<PentState>,
    user: CurrentUser,
    Query(q): Query<ApprovalQuery>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(a) FROM pent_approvals a JOIN pent_runs r ON r.id=a.run_id WHERE r.tenant_id=$1 AND ($2::text IS NULL OR a.status=$2) ORDER BY a.created_at DESC LIMIT $3")
        .bind(tenant).bind(q.status).bind(limit).fetch_all(&s.pool).await.map_err(internal)?;
    Ok(Json(ApiResponse::new(json!(rows))))
}

async fn get_approval(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    let row = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(a) FROM pent_approvals a JOIN pent_runs r ON r.id=a.run_id WHERE a.id=$1 AND r.tenant_id=$2")
        .bind(id).bind(tenant).fetch_optional(&s.pool).await.map_err(internal)?.ok_or_else(|| AppError::not_found("审批不存在"))?;
    Ok(Json(ApiResponse::new(row)))
}

async fn decide_approval(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    let decision = body["decision"].as_str().unwrap_or("");
    if !matches!(decision, "approve" | "reject") {
        return Err(AppError::bad_request("decision 必须是 approve 或 reject"));
    }
    let comment = body["comment"].as_str().unwrap_or("");
    let edited = body.get("arguments").cloned();
    let affected = sqlx::query("UPDATE pent_approvals a SET status='decided',decision=$3,decided_by=$4,decision_comment=$5,effective_arguments=CASE WHEN a.hitl_mode='review_edit' AND $6::jsonb IS NOT NULL THEN $6 ELSE a.effective_arguments END,resumed_at=now(),updated_at=now() FROM pent_runs r WHERE a.id=$1 AND a.run_id=r.id AND r.tenant_id=$2 AND a.status='pending' AND a.expires_at>now()")
        .bind(&id).bind(tenant).bind(decision).bind(&user.user_id).bind(comment).bind(edited)
        .execute(&s.pool).await.map_err(internal)?.rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("审批不存在、已处理或已过期"));
    }
    Ok(Json(ApiResponse::new(json!({"id":id,"decision":decision}))))
}

fn internal(error: sqlx::Error) -> AppError {
    AppError::internal(error.to_string())
}
