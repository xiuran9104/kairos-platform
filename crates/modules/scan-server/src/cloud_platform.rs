use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::*;
use serde_json::{Value, json};

pub async fn zone_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<CloudZone>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_zones t WHERE deleted=0 ORDER BY id",
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
pub async fn zone_get(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<CloudZone>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_zones t WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
pub async fn zone_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateCloudZoneRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_cloud_zones(zone_name,zone_code,description) VALUES($1,$2,$3) RETURNING id")
        .bind(&req.zone_name).bind(&req.zone_code).bind(&req.description)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "云区创建成功", "id": id}),
    )))
}
pub async fn zone_update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateCloudZoneRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cloud_zones SET zone_name=COALESCE($2,zone_name),zone_code=COALESCE($3,zone_code),description=COALESCE($4,description) WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.zone_name).bind(&req.zone_code).bind(&req.description)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "云区更新成功"}))))
}
pub async fn zone_delete(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cloud_zones SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "云区删除成功"}))))
}
pub async fn platform_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<CloudPlatform>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_platforms t WHERE deleted=0 ORDER BY id",
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
pub async fn platform_get(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<CloudPlatform>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_platforms t WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
pub async fn platform_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateCloudPlatformRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_cloud_platforms(zone_id,platform_name,platform_code,description) VALUES($1,$2,$3,$4) RETURNING id")
        .bind(req.zone_id).bind(&req.platform_name).bind(&req.platform_code).bind(&req.description)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "云平台创建成功", "id": id}),
    )))
}
pub async fn platform_update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateCloudPlatformRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cloud_platforms SET zone_id=COALESCE($2,zone_id),platform_name=COALESCE($3,platform_name),platform_code=COALESCE($4,platform_code),description=COALESCE($5,description) WHERE id=$1 AND deleted=0")
        .bind(id).bind(req.zone_id).bind(&req.platform_name).bind(&req.platform_code).bind(&req.description)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "云平台更新成功"}))))
}
pub async fn platform_delete(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cloud_platforms SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "云平台删除成功"}))))
}
