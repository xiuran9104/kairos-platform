use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::*;
use serde_json::{Value, json};

pub async fn list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<CloudProviderConfig>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_provider_configs t WHERE deleted=0 ORDER BY id",
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
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<CloudProviderConfig>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_provider_configs t WHERE id=$1 AND deleted=0",
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
pub async fn create(
    State(s): State<ScanState>,
    Json(req): Json<CreateCloudProviderConfigRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let provider_str = format!("{:?}", req.provider).to_lowercase();
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_cloud_provider_configs(zone_id,platform_id,provider,region_id,region_name,available_zones,account_name,access_key_id,access_key_secret,remarks) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id")
        .bind(req.zone_id).bind(req.platform_id).bind(&provider_str).bind(&req.region_id)
        .bind(&req.region_name).bind(&req.available_zones.join(",")).bind(&req.account_name)
        .bind(&req.access_key_id).bind(&req.access_key_secret).bind(&req.remarks)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "云厂商对接创建成功", "id": id}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateCloudProviderConfigRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let zones = req.available_zones.map(|z| z.join(","));
    sqlx::query("UPDATE scan_cloud_provider_configs SET region_name=COALESCE($2,region_name),account_name=COALESCE($3,account_name),access_key_id=COALESCE($4,access_key_id),access_key_secret=COALESCE($5,access_key_secret),available_zones=COALESCE($6,available_zones),remarks=COALESCE($7,remarks),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.region_name).bind(&req.account_name).bind(&req.access_key_id)
        .bind(&req.access_key_secret).bind(&zones).bind(&req.remarks)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "云厂商对接更新成功"}),
    )))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_cloud_provider_configs SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "云厂商对接删除成功"}),
    )))
}
