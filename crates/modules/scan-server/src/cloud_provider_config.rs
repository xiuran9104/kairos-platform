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
        "SELECT to_jsonb(t) || jsonb_build_object('access_key_secret','','auth_password',NULL,'available_zones', CASE WHEN COALESCE(t.available_zones, '') = '' THEN '[]'::jsonb ELSE to_jsonb(string_to_array(t.available_zones, ',')) END) FROM scan_cloud_provider_configs t WHERE deleted=0 ORDER BY id",
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
        "SELECT to_jsonb(t) || jsonb_build_object('access_key_secret','','auth_password',NULL,'available_zones', CASE WHEN COALESCE(t.available_zones, '') = '' THEN '[]'::jsonb ELSE to_jsonb(string_to_array(t.available_zones, ',')) END) FROM scan_cloud_provider_configs t WHERE id=$1 AND deleted=0",
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
    let id: i64 = sqlx::query_scalar("INSERT INTO scan_cloud_provider_configs(platform_id,provider,region_id,region_name,available_zones,account_name,access_key_id,access_key_secret,remarks,auth_type,auth_username,auth_password,auth_domain,project_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING id")
        .bind(req.platform_id).bind(&provider_str).bind(&req.region_id)
        .bind(&req.region_name).bind(req.available_zones.as_deref().unwrap_or(&[]).join(",")).bind(&req.account_name)
        .bind(&req.access_key_id).bind(&req.access_key_secret).bind(&req.remarks).bind(req.auth_type.as_deref().unwrap_or("access_key"))
        .bind(&req.auth_username).bind(&req.auth_password).bind(&req.auth_domain).bind(&req.project_id)
        .fetch_one(&s.pool).await.map_err(|e| AppError::bad_request(e.to_string()))?;
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
    let provider = req
        .provider
        .map(|value| format!("{value:?}").to_lowercase());
    sqlx::query("UPDATE scan_cloud_provider_configs SET platform_id=COALESCE($2,platform_id),provider=COALESCE($3,provider),region_id=COALESCE($4,region_id),region_name=COALESCE($5,region_name),account_name=COALESCE($6,account_name),access_key_id=COALESCE($7,access_key_id),access_key_secret=COALESCE(NULLIF($8,''),access_key_secret),available_zones=COALESCE($9,available_zones),status=COALESCE($10,status),remarks=COALESCE($11,remarks),auth_type=COALESCE($12,auth_type),auth_username=COALESCE($13,auth_username),auth_password=COALESCE(NULLIF($14,''),auth_password),auth_domain=COALESCE($15,auth_domain),project_id=COALESCE($16,project_id),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(req.platform_id).bind(&provider).bind(&req.region_id).bind(&req.region_name)
        .bind(&req.account_name).bind(&req.access_key_id).bind(&req.access_key_secret)
        .bind(&zones).bind(req.status.map(|value| format!("{value:?}").to_lowercase())).bind(&req.remarks)
        .bind(&req.auth_type).bind(&req.auth_username).bind(&req.auth_password).bind(&req.auth_domain).bind(&req.project_id)
        .execute(&s.pool).await.map_err(|e| AppError::bad_request(e.to_string()))?;
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
