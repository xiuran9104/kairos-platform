use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
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
pub async fn approval_history(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows=sqlx::query_scalar("SELECT to_jsonb(a) FROM scan_resource_ticket_approvals a WHERE ticket_id=$1 ORDER BY stage_no")
        .bind(id).fetch_all(&s.pool).await.map_err(|e|AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
pub async fn create(
    State(s): State<ScanState>,
    Json(req): Json<CreateResourceTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::resource_ticket_validation::validate_business_resource_type(&req.resource_type)?;
    let ticket_type = req.ticket_type.as_deref().unwrap_or("create");
    let approval = crate::resource_ticket_approval::policy(ticket_type);
    if approval.requires_target && req.target_resource_id.is_none() {
        return Err(AppError::bad_request("该工单类型必须选择目标业务资源"));
    }
    if approval.requires_window && req.maintenance_window.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::bad_request("高风险操作必须填写维护窗口"));
    }
    if approval.requires_backup && !req.backup_confirmed.unwrap_or(false) {
        return Err(AppError::bad_request("高风险操作必须确认数据备份"));
    }
    if approval.requires_rollback && req.rollback_plan.as_deref().unwrap_or("").trim().is_empty() {
        return Err(AppError::bad_request("高风险操作必须填写回滚方案"));
    }
    if ticket_type == "reclaim" && req.retention_until.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::bad_request("资源回收必须设置观察保留期"));
    }
    if ticket_type == "create" && req.resource_type == "cloud" && req.cloud_platform_id.is_none() {
        return Err(AppError::bad_request("云资源工单必须选择云平台"));
    }
    let cloud_platform_id = (req.resource_type == "cloud")
        .then_some(req.cloud_platform_id)
        .flatten();
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let id: i64 = sqlx::query_scalar("INSERT INTO scan_resource_tickets(resource_type,ecs_name,provider_id,cloud_platform_id,machine_room_id,cloud_region,cloud_category,zone_name,zone_cabinet,rack_units,customer_name,application_name,contract_name,ecs_type,ecs_os,resource_count,cpu_cores,memory_gb,system_disk,system_disk_size_gb,data_disk,expire_at,has_security_product,security_products,ip_address,remarks,created_by,applicant_name) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22::timestamptz,$23,$24,$25,$26,'admin','admin') RETURNING id")
        .bind(&req.resource_type).bind(&req.ecs_name).bind(req.provider_id).bind(cloud_platform_id)
        .bind(req.machine_room_id).bind(&req.cloud_region).bind(&req.cloud_category)
        .bind(&req.zone_name).bind(&req.zone_cabinet).bind(req.rack_units.unwrap_or(0)).bind(&req.customer_name)
        .bind(&req.application_name).bind(&req.contract_name).bind(&req.ecs_type).bind(&req.ecs_os)
        .bind(req.resource_count.unwrap_or(1)).bind(req.cpu_cores.unwrap_or(0)).bind(req.memory_gb.unwrap_or(0))
        .bind(&req.system_disk).bind(req.system_disk_size_gb.unwrap_or(0)).bind(&req.data_disk)
        .bind(&req.expire_at).bind(req.has_security_product.unwrap_or(false))
        .bind(&req.security_products).bind(&req.ip_address).bind(&req.remarks)
        .fetch_one(&mut *tx).await.map_err(|e| AppError::bad_request(e.to_string()))?;
    sqlx::query("UPDATE scan_resource_tickets SET ticket_type=$2,risk_level=$3,target_resource_id=$4,target_config=$5,maintenance_window=$6::timestamptz,allow_interruption=$7,backup_confirmed=$8,rollback_plan=$9,retention_until=$10::timestamptz,approval_stage=1,approval_total=$11,current_approval_role=$12 WHERE id=$1")
        .bind(id).bind(ticket_type).bind(approval.risk_level).bind(req.target_resource_id).bind(&req.target_config).bind(&req.maintenance_window)
        .bind(req.allow_interruption.unwrap_or(false)).bind(req.backup_confirmed.unwrap_or(false)).bind(&req.rollback_plan).bind(&req.retention_until)
        .bind(approval.roles.len() as i32).bind(approval.roles.first().copied().unwrap_or("审批人")).execute(&mut *tx).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
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
    let result=sqlx::query("UPDATE scan_resource_tickets SET deleted=1 WHERE id=$1 AND deleted=0 AND ticket_status='rejected'")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    if result.rows_affected() != 1 {
        return Err(AppError::bad_request(
            "仅允许删除已拒绝的工单，流程记录必须保留",
        ));
    }
    Ok(Json(ApiResponse::new(json!({"message": "工单删除成功"}))))
}
pub async fn approve(
    State(s): State<ScanState>,
    user: CurrentUser,
    Path(id): Path<i32>,
    Json(req): Json<ApproveTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let result = crate::resource_ticket_approval::approve(&s, &user, id, req).await?;
    Ok(Json(ApiResponse::new(result)))
}
pub async fn provision(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<ProvisionTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    if req.config_id.is_some() {
        return crate::cloud_provisioning::provision_cloud_ticket(&s, id, req).await;
    }
    let current: Option<String> = sqlx::query_scalar(
        "SELECT ticket_status FROM scan_resource_tickets WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if !matches!(current.as_deref(), Some("pending_provision" | "approved")) {
        return Err(AppError::bad_request("只能配置待配置状态的工单"));
    }
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status='pending_delivery',provisioner='admin',provision_time=now(),provision_details=$2 WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.details).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "配置中"}))))
}
pub async fn deliver(
    State(s): State<ScanState>,
    Path(id): Path<i32>,
    Json(req): Json<DeliverTicketRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let current: Option<String> = sqlx::query_scalar(
        "SELECT ticket_status FROM scan_resource_tickets WHERE id=$1 AND deleted=0",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if current.as_deref() != Some("pending_delivery") {
        return Err(AppError::bad_request("只能交付待交付状态的工单"));
    }
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status='delivered',deliverer='admin',deliver_time=now(),deliver_comment=$2 WHERE id=$1 AND deleted=0")
        .bind(id).bind(&req.comment).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message": "交付完成"}))))
}
