use crate::ScanState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{CreateInfrastructureDeviceRequest, InfrastructureDevice};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct DeviceFilter {
    device_type: String,
}

fn validate_type(value: &str) -> Result<(), AppError> {
    if ["network", "storage"].contains(&value) {
        Ok(())
    } else {
        Err(AppError::bad_request("设备类型仅支持 network 或 storage"))
    }
}

pub async fn list(
    State(s): State<ScanState>,
    Query(filter): Query<DeviceFilter>,
) -> Result<Json<ApiResponse<Vec<InfrastructureDevice>>>, AppError> {
    validate_type(&filter.device_type)?;
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_infrastructure_devices t WHERE deleted=0 AND device_type=$1 ORDER BY id DESC")
        .bind(&filter.device_type).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
    )))
}

pub async fn create(
    State(s): State<ScanState>,
    Json(req): Json<CreateInfrastructureDeviceRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    validate_type(&req.device_type)?;
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_infrastructure_devices(device_type,name,category,vendor,model,serial_number,management_ip,business_ip,machine_room_id,rack_position,status,specification,capacity,contact_person,contact_phone,remarks) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING id")
        .bind(&req.device_type).bind(&req.name).bind(&req.category).bind(&req.vendor).bind(&req.model).bind(&req.serial_number)
        .bind(&req.management_ip).bind(&req.business_ip).bind(req.machine_room_id).bind(&req.rack_position)
        .bind(req.status.as_deref().unwrap_or("active")).bind(&req.specification).bind(&req.capacity)
        .bind(&req.contact_person).bind(&req.contact_phone).bind(&req.remarks)
        .fetch_one(&s.pool).await.map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"基础设施设备创建成功","id":id}),
    )))
}

pub async fn update(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<CreateInfrastructureDeviceRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    validate_type(&req.device_type)?;
    sqlx::query("UPDATE scan_infrastructure_devices SET name=$2,category=$3,vendor=$4,model=$5,serial_number=$6,management_ip=$7,business_ip=$8,machine_room_id=$9,rack_position=$10,status=$11,specification=$12,capacity=$13,contact_person=$14,contact_phone=$15,remarks=$16,updated_at=now() WHERE id=$1 AND device_type=$17 AND deleted=0")
        .bind(id).bind(&req.name).bind(&req.category).bind(&req.vendor).bind(&req.model).bind(&req.serial_number)
        .bind(&req.management_ip).bind(&req.business_ip).bind(req.machine_room_id).bind(&req.rack_position)
        .bind(req.status.as_deref().unwrap_or("active")).bind(&req.specification).bind(&req.capacity)
        .bind(&req.contact_person).bind(&req.contact_phone).bind(&req.remarks).bind(&req.device_type)
        .execute(&s.pool).await.map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"基础设施设备更新成功"}),
    )))
}

pub async fn delete_one(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE scan_infrastructure_devices SET deleted=1,updated_at=now() WHERE id=$1 AND deleted=0").bind(id)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"基础设施设备删除成功"}),
    )))
}
