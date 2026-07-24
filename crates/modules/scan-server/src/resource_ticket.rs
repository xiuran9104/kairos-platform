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
) -> Result<Json<ApiResponse<Vec<ResourceTicket>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_resource_tickets t WHERE deleted=0 ORDER BY id DESC",
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
) -> Result<Json<ApiResponse<ResourceTicket>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_resource_tickets t WHERE id=$1 AND deleted=0",
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
    Json(req): Json<CreateResourceTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_resource_tickets(resource_type,ecs_name,cloud_region,cloud_category,zone_name,rack_units,customer_name,application_name,contract_name,ecs_type,ecs_os,resource_count,cpu_cores,memory_gb,system_disk,system_disk_size_gb,data_disk,expire_at,has_security_product,security_products,ip_address,remarks) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22) RETURNING id")
        .bind(&req.resource_type).bind(&req.ecs_name).bind(&req.cloud_region).bind(&req.cloud_category)
        .bind(&req.zone_name).bind(req.rack_units.unwrap_or(0)).bind(&req.customer_name)
        .bind(&req.application_name).bind(&req.contract_name).bind(&req.ecs_type).bind(&req.ecs_os)
        .bind(req.resource_count.unwrap_or(1)).bind(req.cpu_cores.unwrap_or(0)).bind(req.memory_gb.unwrap_or(0))
        .bind(&req.system_disk).bind(req.system_disk_size_gb.unwrap_or(0)).bind(&req.data_disk)
        .bind(&req.expire_at).bind(req.has_security_product.unwrap_or(false))
        .bind(&req.security_products).bind(&req.ip_address).bind(&req.remarks)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "工单创建成功", "id": id}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateResourceTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_resource_tickets SET ecs_name=COALESCE($2,ecs_name),cloud_region=COALESCE($3,cloud_region),cloud_category=COALESCE($4,cloud_category),customer_name=COALESCE($5,customer_name),ecs_type=COALESCE($6,ecs_type),ecs_os=COALESCE($7,ecs_os),remarks=COALESCE($8,remarks),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.ecs_name).bind(&req.cloud_region).bind(&req.cloud_category)
        .bind(&req.customer_name).bind(&req.ecs_type).bind(&req.ecs_os).bind(&req.remarks)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "工单更新成功"}))))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_resource_tickets SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "工单删除成功"}))))
}
pub async fn approve(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<ApproveTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status=$2,approver='admin',approve_time=now(),approve_comment=$3 WHERE id=$1 AND deleted=0")
        .bind(id).bind(if req.approved { "approved" } else { "rejected" }).bind(&req.comment)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "审批完成"}))))
}
pub async fn provision(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<ProvisionTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status='provisioning',provisioner='admin',provision_time=now(),provision_details=$2 WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.details).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "配置中"}))))
}
pub async fn deliver(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<DeliverTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status='delivered',deliverer='admin',deliver_time=now(),deliver_comment=$2 WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.comment).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "交付完成"}))))
}
