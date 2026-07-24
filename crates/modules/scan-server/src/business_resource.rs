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
) -> Result<Json<ApiResponse<Vec<BusinessResource>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_business_resources t WHERE deleted=0 ORDER BY id DESC",
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
) -> Result<Json<ApiResponse<BusinessResource>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_business_resources t WHERE id=$1 AND deleted=0",
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
    Json(req): Json<CreateBusinessResourceRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_business_resources(resource_type,ecs_name,ecs_status,cloud_region,cloud_category,customer_name,instance_id,ecs_type,ecs_os,cpu_cores,memory_gb,system_disk,system_disk_size_gb,ip_address,application_status,delivery_status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING id")
        .bind(&req.resource_type).bind(&req.ecs_name).bind(&req.ecs_status).bind(&req.cloud_region)
        .bind(&req.cloud_category).bind(&req.customer_name).bind(req.instance_id.as_deref().unwrap_or(""))
        .bind(&req.ecs_type).bind(&req.ecs_os).bind(req.cpu_cores as i32).bind(req.memory_gb as i32)
        .bind(&req.system_disk).bind(req.system_disk_size_gb as i32).bind(&req.ip_address)
        .bind(req.application_status.as_deref().unwrap_or("待审核")).bind(req.delivery_status.as_deref().unwrap_or("待交付"))
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "业务资源创建成功", "id": id}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateBusinessResourceRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_business_resources SET ecs_name=COALESCE($2,ecs_name),ecs_status=COALESCE($3,ecs_status),cloud_region=COALESCE($4,cloud_region),cloud_category=COALESCE($5,cloud_category),customer_name=COALESCE($6,customer_name),ecs_type=COALESCE($7,ecs_type),ecs_os=COALESCE($8,ecs_os),application_status=COALESCE($9,application_status),delivery_status=COALESCE($10,delivery_status),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.ecs_name).bind(&req.ecs_status).bind(&req.cloud_region).bind(&req.cloud_category)
        .bind(&req.customer_name).bind(&req.ecs_type).bind(&req.ecs_os)
        .bind(&req.application_status).bind(&req.delivery_status)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "业务资源更新成功"}),
    )))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_business_resources SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "业务资源删除成功"}),
    )))
}
