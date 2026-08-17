use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct UpdateRiskRequest {
    pub status: Option<RiskStatus>,
    pub assigned_to: Option<String>,
    pub solution: Option<String>,
}

pub async fn list(State(s): State<ScanState>) -> Result<Json<ApiResponse<Vec<Risk>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_risks t WHERE deleted=0 ORDER BY created_at DESC",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
    )))
}
pub async fn get_one(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Risk>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_risks t WHERE id=$1 AND deleted=0",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
pub async fn update_one(
    State(s): State<ScanState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRiskRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let status_str = req.status.as_ref().map(|status| match status {
        RiskStatus::Open => "open",
        RiskStatus::Verified => "verified",
        RiskStatus::Resolved => "resolved",
        RiskStatus::Ignored => "ignored",
        RiskStatus::FalsePositive => "false_positive",
        RiskStatus::PendingReview => "pending_review",
    });
    sqlx::query("UPDATE scan_risks SET status=COALESCE($2,status),assigned_to=COALESCE($3,assigned_to),solution=COALESCE($4,solution),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(&id).bind(&status_str).bind(&req.assigned_to).bind(&req.solution)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "风险更新成功"}))))
}
