use crate::{ScanState, cloud_adapter};
use axum::Json;
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::ProvisionTicketRequest;
use serde_json::{Value, json};

pub async fn provision_cloud_ticket(
    state: &ScanState,
    ticket_id: i32,
    request: ProvisionTicketRequest,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let ticket: (String, String, String, i32, i32, String, String, i32, Option<i64>) = sqlx::query_as(
        "SELECT resource_type,ecs_name,ticket_status,COALESCE(cpu_cores,0),COALESCE(memory_gb,0),COALESCE(ecs_os,''),COALESCE(customer_name,''),COALESCE(system_disk_size_gb,100),cloud_platform_id FROM scan_resource_tickets WHERE id=$1 AND deleted=0",
    )
    .bind(ticket_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("工单不存在"))?;
    if ticket.0 != "cloud" {
        return Err(AppError::bad_request("只有云资源工单可以自动开通"));
    }
    if !matches!(ticket.2.as_str(), "pending_provision" | "approved") {
        return Err(AppError::bad_request("只能开通待配置状态的工单"));
    }
    let config_id = request
        .config_id
        .ok_or_else(|| AppError::bad_request("请选择云厂商对接配置"))?;
    let image_id = request
        .image_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("请输入镜像 ID"))?;
    let flavor = request
        .flavor
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("请输入实例规格"))?;
    let config_matches: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM scan_cloud_provider_configs WHERE id=$1 AND platform_id=$2 AND status='active' AND deleted=0)")
        .bind(config_id).bind(ticket.8).fetch_one(&state.pool).await.map_err(|error| AppError::internal(error.to_string()))?;
    if !config_matches {
        return Err(AppError::bad_request(
            "对接配置不属于工单所选云平台或尚未启用",
        ));
    }
    let claimed = sqlx::query("UPDATE scan_resource_tickets SET ticket_status='provisioning',provisioner='admin',provision_time=now() WHERE id=$1 AND ticket_status IN ('pending_provision','approved')")
        .bind(ticket_id).execute(&state.pool).await.map_err(|error| AppError::internal(error.to_string()))?;
    if claimed.rows_affected() != 1 {
        return Err(AppError::bad_request("工单已被处理，请刷新后重试"));
    }
    let credentials = match cloud_adapter::resolve_creds_inner(state, config_id).await {
        Ok(value) => value,
        Err(error) => {
            reset_failed(state, ticket_id, "认证配置不可用").await?;
            return Err(error);
        }
    };
    let adapter = cloud_adapter::get_adapter_inner(&credentials.kind);
    let instance = match adapter
        .create_instance(
            &credentials,
            &ticket.1,
            image_id,
            flavor,
            ticket.7.max(20) as u32,
        )
        .await
    {
        Ok(instance) => instance,
        Err(error) => {
            reset_failed(state, ticket_id, &error).await?;
            return Err(AppError::internal(error));
        }
    };
    if instance.instance_id.trim().is_empty() {
        sqlx::query("UPDATE scan_resource_tickets SET ticket_status='pending_provision',provision_details='自动开通失败: 云平台未返回实例 ID' WHERE id=$1")
            .bind(ticket_id).execute(&state.pool).await.map_err(|error| AppError::internal(error.to_string()))?;
        return Err(AppError::internal("云平台未返回实例 ID，未写入资源台账"));
    }
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query("INSERT INTO scan_business_resources(resource_type,ecs_name,ecs_status,cloud_region,cloud_category,customer_name,instance_id,ecs_type,ecs_os,cpu_cores,memory_gb,system_disk,system_disk_size_gb,ip_address,application_status,delivery_status,cloud_provider_config_id) VALUES('cloud',$1,$2,$3,'',$4,$5,$6,$7,$8,$9,'云硬盘',$10,$11,'已批准','待交付',$12)")
        .bind(&ticket.1).bind(&instance.status).bind(credentials.region.as_deref().unwrap_or("")).bind(&ticket.6)
        .bind(&instance.instance_id).bind(flavor).bind(&ticket.5).bind(ticket.3).bind(ticket.4)
        .bind(instance.system_disk_gb as i32).bind(instance.private_ip.first().map(String::as_str).unwrap_or("")).bind(config_id)
        .execute(&mut *transaction).await.map_err(|error| AppError::internal(error.to_string()))?;
    let details = request
        .details
        .unwrap_or_else(|| format!("云实例已创建，实例 ID: {}", instance.instance_id));
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status='pending_delivery',provision_details=$2,ip_address=$3 WHERE id=$1")
        .bind(ticket_id).bind(&details).bind(instance.private_ip.first()).execute(&mut *transaction).await.map_err(|error| AppError::internal(error.to_string()))?;
    transaction
        .commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"云资源开通成功","instance":instance}),
    )))
}

async fn reset_failed(state: &ScanState, ticket_id: i32, reason: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE scan_resource_tickets SET ticket_status='pending_provision',provision_details=$2 WHERE id=$1")
        .bind(ticket_id).bind(format!("自动开通失败: {reason}")).execute(&state.pool).await.map_err(|error| AppError::internal(error.to_string()))?;
    Ok(())
}
