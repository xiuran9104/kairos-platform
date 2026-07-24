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
pub struct UpdatePortDetailRequest {
    pub system_name: Option<String>,
    pub system_type: Option<String>,
    pub middleware: Option<String>,
    pub framework: Option<String>,
    pub language: Option<String>,
    pub environment: Option<String>,
    pub department: Option<String>,
    pub owner: Option<String>,
    pub owner_contact: Option<String>,
    pub notes: Option<String>,
    pub is_bound: Option<bool>,
}

pub async fn list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<PortDetail>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_port_details t WHERE deleted=0 ORDER BY created_at DESC LIMIT 500")
        .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
    )))
}
pub async fn get_one(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<PortDetail>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_port_details t WHERE id=$1 AND deleted=0",
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
    Json(req): Json<UpdatePortDetailRequest>,
) -> Result<Json<ApiResponse<PortDetail>>, AppError> {
    sqlx::query("UPDATE scan_port_details SET system_name=COALESCE($2,system_name),system_type=COALESCE($3,system_type),middleware=COALESCE($4,middleware),framework=COALESCE($5,framework),language=COALESCE($6,language),environment=COALESCE($7,environment),department=COALESCE($8,department),owner=COALESCE($9,owner),owner_contact=COALESCE($10,owner_contact),notes=COALESCE($11,notes),is_bound=COALESCE($12,is_bound),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(&id).bind(&req.system_name).bind(&req.system_type).bind(&req.middleware)
        .bind(&req.framework).bind(&req.language).bind(&req.environment).bind(&req.department)
        .bind(&req.owner).bind(&req.owner_contact).bind(&req.notes).bind(req.is_bound)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    get_one(State(s), Path(id)).await
}
