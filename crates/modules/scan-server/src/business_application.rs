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
) -> Result<Json<ApiResponse<Vec<BusinessApplication>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_business_applications t WHERE deleted=0 ORDER BY id",
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
) -> Result<Json<ApiResponse<BusinessApplication>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_business_applications t WHERE id=$1 AND deleted=0",
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
    Json(req): Json<CreateBusinessApplicationRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO scan_business_applications(name,description) VALUES($1,$2) RETURNING id",
    )
    .bind(&req.name)
    .bind(&req.description)
    .fetch_one(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "业务应用创建成功", "id": id}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<CreateBusinessApplicationRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_business_applications SET name=$2,description=$3,updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.name).bind(&req.description).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "业务应用更新成功"}),
    )))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_business_applications SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "业务应用删除成功"}),
    )))
}
pub async fn endpoints(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<ApplicationEndpoint>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_application_endpoints t WHERE business_application_id=$1 AND deleted=0")
        .bind(id).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
    )))
}
pub async fn add_endpoint(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<CreateApplicationEndpointRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let eid: i32 = sqlx::query_scalar("INSERT INTO scan_application_endpoints(business_application_id,protocol,dest_ip,nat_ip,dest_port,domain) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(id).bind(&req.protocol).bind(&req.dest_ip).bind(&req.nat_ip).bind(&req.dest_port).bind(&req.domain)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "接口添加成功", "id": eid}),
    )))
}
