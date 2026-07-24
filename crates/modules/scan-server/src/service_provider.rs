use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{
    CreateServiceProviderRequest, ServiceProvider, UpdateServiceProviderRequest,
};
use serde_json::{Value, json};

pub async fn list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<ServiceProvider>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_service_providers t WHERE deleted=0 ORDER BY id",
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
) -> Result<Json<ApiResponse<ServiceProvider>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_service_providers t WHERE id=$1 AND deleted=0",
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
    Json(req): Json<CreateServiceProviderRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_service_providers(provider_name,provider_code,short_name,logo_url,contact_person,contact_phone,contact_email,headquarters,service_area,business_license,remarks,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING id")
        .bind(&req.provider_name).bind(&req.provider_code).bind(&req.short_name).bind(&req.logo_url)
        .bind(&req.contact_person).bind(&req.contact_phone).bind(&req.contact_email).bind(&req.headquarters)
        .bind(&req.service_area).bind(&req.business_license).bind(&req.remarks)
        .bind(req.status.as_deref().unwrap_or("active"))
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "服务商创建成功", "id": id}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateServiceProviderRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_service_providers SET provider_name=COALESCE($2,provider_name),provider_code=COALESCE($3,provider_code),short_name=COALESCE($4,short_name),contact_person=COALESCE($5,contact_person),contact_phone=COALESCE($6,contact_phone),contact_email=COALESCE($7,contact_email),headquarters=COALESCE($8,headquarters),service_area=COALESCE($9,service_area),business_license=COALESCE($10,business_license),remarks=COALESCE($11,remarks),status=COALESCE($12,status),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.provider_name).bind(&req.provider_code).bind(&req.short_name)
        .bind(&req.contact_person).bind(&req.contact_phone).bind(&req.contact_email)
        .bind(&req.headquarters).bind(&req.service_area).bind(&req.business_license)
        .bind(&req.remarks).bind(&req.status)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "服务商更新成功"}))))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_service_providers SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "服务商删除成功"}))))
}
