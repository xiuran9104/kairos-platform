use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{
    CreateSecurityProductRequest, SecurityProduct, UpdateSecurityProductRequest,
};
use serde_json::{Value, json};

pub async fn list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<SecurityProduct>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_security_products t WHERE deleted=0 ORDER BY id",
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
) -> Result<Json<ApiResponse<SecurityProduct>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_security_products t WHERE id=$1 AND deleted=0",
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
    Json(req): Json<CreateSecurityProductRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let features = req
        .features
        .as_ref()
        .map(|f| serde_json::to_string(f).unwrap_or_default());
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_security_products(name,category,vendor,model,version,serial_number,license_type,license_expiry,management_ip,deployment_mode,cloud_platform_id,machine_room_id,provider_id,status,features,throughput,contact_person,contact_phone,remarks) VALUES($1,$2,$3,$4,$5,$6,$7,$8::date,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) RETURNING id")
        .bind(&req.name).bind(&req.category).bind(&req.vendor).bind(&req.model).bind(&req.version)
        .bind(&req.serial_number).bind(&req.license_type).bind(&req.license_expiry).bind(&req.management_ip).bind(&req.deployment_mode)
        .bind(req.cloud_platform_id).bind(req.machine_room_id).bind(req.provider_id.unwrap_or(0))
        .bind(req.status.as_deref().unwrap_or("active")).bind(&features).bind(&req.throughput)
        .bind(&req.contact_person).bind(&req.contact_phone).bind(&req.remarks)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "安全产品创建成功", "id": id}),
    )))
}
pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateSecurityProductRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let features = req
        .features
        .as_ref()
        .map(|f| serde_json::to_string(f).unwrap_or_default());
    sqlx::query("UPDATE scan_security_products SET name=COALESCE($2,name),category=COALESCE($3,category),vendor=COALESCE($4,vendor),model=COALESCE($5,model),version=COALESCE($6,version),license_type=COALESCE($7,license_type),deployment_mode=COALESCE($8,deployment_mode),status=COALESCE($9,status),features=COALESCE($10,features),throughput=COALESCE($11,throughput),contact_person=COALESCE($12,contact_person),contact_phone=COALESCE($13,contact_phone),remarks=COALESCE($14,remarks),serial_number=COALESCE($15,serial_number),license_expiry=COALESCE($16::date,license_expiry),management_ip=COALESCE($17,management_ip),cloud_platform_id=COALESCE($18,cloud_platform_id),machine_room_id=COALESCE($19,machine_room_id),provider_id=COALESCE($20,provider_id),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.name).bind(&req.category).bind(&req.vendor).bind(&req.model).bind(&req.version)
        .bind(&req.license_type).bind(&req.deployment_mode).bind(&req.status).bind(&features)
        .bind(&req.throughput).bind(&req.contact_person).bind(&req.contact_phone).bind(&req.remarks)
        .bind(&req.serial_number).bind(&req.license_expiry).bind(&req.management_ip).bind(req.cloud_platform_id).bind(req.machine_room_id).bind(req.provider_id)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "安全产品更新成功"}),
    )))
}
pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_security_products SET deleted=1 WHERE id=$1 AND deleted=0")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message": "安全产品删除成功"}),
    )))
}
