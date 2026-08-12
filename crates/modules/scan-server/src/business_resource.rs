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
    crate::physical_resource::validate_status(&req.resource_type, &req.ecs_status)?;
    let deployment_type = crate::physical_resource::validate(
        &req.resource_type,
        req.deployment_type.as_deref(),
        req.machine_room_id,
        req.management_ip.as_deref(),
        req.business_ip.as_deref(),
        req.network_cidr.as_deref(),
        req.gateway.as_deref(),
    )?;
    if req.resource_type == "physical" {
        crate::physical_resource::validate_placement(
            &deployment_type,
            req.cabinet_id,
            req.rack_start_u,
            req.rack_end_u,
        )?;
        crate::physical_resource::validate_cloud_binding(
            &s.pool,
            &deployment_type,
            req.cloud_provider_config_id,
            req.machine_room_id,
        )
        .await?;
        crate::physical_resource::validate_cabinet_placement(
            &s.pool,
            &deployment_type,
            req.machine_room_id,
            req.cabinet_id,
            req.rack_start_u,
            req.rack_end_u,
            None,
        )
        .await?;
    }
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_business_resources(resource_type,ecs_name,ecs_status,cloud_region,cloud_category,customer_name,instance_id,ecs_type,ecs_os,cpu_cores,memory_gb,system_disk,system_disk_size_gb,ip_address,application_status,delivery_status,machine_room_id,deployment_type,management_ip,business_ip,network_cidr,gateway,vlan_id,dns_servers,mac_address,switch_name,switch_port,cloud_provider_config_id,cabinet_name,chassis_name,chassis_slot,rack_start_u,rack_end_u,cabinet_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34) RETURNING id")
        .bind(&req.resource_type).bind(&req.ecs_name).bind(&req.ecs_status).bind(&req.cloud_region)
        .bind(&req.cloud_category).bind(&req.customer_name).bind(req.instance_id.as_deref().unwrap_or(""))
        .bind(&req.ecs_type).bind(&req.ecs_os).bind(req.cpu_cores as i32).bind(req.memory_gb as i32)
        .bind(&req.system_disk).bind(req.system_disk_size_gb as i32).bind(&req.ip_address)
        .bind(req.application_status.as_deref().unwrap_or("待审核")).bind(req.delivery_status.as_deref().unwrap_or("待交付")).bind(req.machine_room_id)
        .bind(deployment_type).bind(&req.management_ip).bind(&req.business_ip).bind(&req.network_cidr).bind(&req.gateway)
        .bind(&req.vlan_id).bind(&req.dns_servers).bind(&req.mac_address).bind(&req.switch_name).bind(&req.switch_port)
        .bind(req.cloud_provider_config_id)
        .bind(&req.cabinet_name).bind(&req.chassis_name).bind(&req.chassis_slot).bind(req.rack_start_u).bind(req.rack_end_u)
        .bind(req.cabinet_id)
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
    let resource_type = req.resource_type.as_deref().unwrap_or("physical");
    if let Some(status) = req.ecs_status.as_deref() {
        crate::physical_resource::validate_status(resource_type, status)?;
    }
    crate::physical_resource::validate(
        resource_type,
        req.deployment_type.as_deref(),
        req.machine_room_id,
        req.management_ip.as_deref(),
        req.business_ip.as_deref(),
        req.network_cidr.as_deref(),
        req.gateway.as_deref(),
    )?;
    let deployment_type = req
        .deployment_type
        .as_deref()
        .unwrap_or(crate::physical_resource::STANDALONE);
    if resource_type == "physical" {
        crate::physical_resource::validate_placement(
            deployment_type,
            req.cabinet_id,
            req.rack_start_u,
            req.rack_end_u,
        )?;
        crate::physical_resource::validate_cloud_binding(
            &s.pool,
            deployment_type,
            req.cloud_provider_config_id,
            req.machine_room_id,
        )
        .await?;
        crate::physical_resource::validate_cabinet_placement(
            &s.pool,
            deployment_type,
            req.machine_room_id,
            req.cabinet_id,
            req.rack_start_u,
            req.rack_end_u,
            Some(id),
        )
        .await?;
    }
    sqlx::query("UPDATE scan_business_resources SET ecs_name=COALESCE($2,ecs_name),ecs_status=COALESCE($3,ecs_status),cloud_region=COALESCE($4,cloud_region),cloud_category=COALESCE($5,cloud_category),customer_name=COALESCE($6,customer_name),ecs_type=COALESCE($7,ecs_type),ecs_os=COALESCE($8,ecs_os),application_status=COALESCE($9,application_status),delivery_status=COALESCE($10,delivery_status),machine_room_id=CASE WHEN $12='standalone' THEN NULL ELSE COALESCE($11,machine_room_id) END,deployment_type=COALESCE($12,deployment_type),management_ip=$13,business_ip=$14,network_cidr=$15,gateway=$16,vlan_id=$17,dns_servers=$18,mac_address=$19,switch_name=$20,switch_port=$21,cloud_provider_config_id=CASE WHEN $12='cloud_managed' THEN $22 ELSE NULL END,cabinet_name=$23,chassis_name=$24,chassis_slot=$25,rack_start_u=$26,rack_end_u=$27,cabinet_id=$28,updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.ecs_name).bind(&req.ecs_status).bind(&req.cloud_region).bind(&req.cloud_category)
        .bind(&req.customer_name).bind(&req.ecs_type).bind(&req.ecs_os)
        .bind(&req.application_status).bind(&req.delivery_status).bind(req.machine_room_id).bind(&req.deployment_type)
        .bind(&req.management_ip).bind(&req.business_ip).bind(&req.network_cidr).bind(&req.gateway).bind(&req.vlan_id)
        .bind(&req.dns_servers).bind(&req.mac_address).bind(&req.switch_name).bind(&req.switch_port)
        .bind(req.cloud_provider_config_id)
        .bind(&req.cabinet_name).bind(&req.chassis_name).bind(&req.chassis_slot).bind(req.rack_start_u).bind(req.rack_end_u)
        .bind(req.cabinet_id)
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
