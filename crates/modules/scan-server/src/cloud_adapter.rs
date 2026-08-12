//! Cloud adapter — common trait and dispatcher for multi-cloud management.
//!
//! Supported platforms: OpenStack, Huawei HCS, Aliyun Apsara Stack, TStack.

use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{
    CloudCreateInstanceRequest, CloudCredentials, CloudInstance, CloudInstanceActionRequest,
    CloudPlatformKind, CloudRegion, CloudSecurityGroup, CloudSyncRequest, CloudSyncResult,
    CloudTestResult, CloudVpc,
};
use serde_json::{Value, json};

use crate::ScanState;

/// Trait for cloud platform adapters.
#[async_trait::async_trait]
pub trait CloudApi: Send + Sync {
    fn kind(&self) -> CloudPlatformKind;
    async fn test_connection(&self, creds: &CloudCredentials) -> Result<CloudTestResult, String>;
    async fn list_instances(&self, creds: &CloudCredentials) -> Result<Vec<CloudInstance>, String>;
    async fn get_instance(
        &self,
        creds: &CloudCredentials,
        id: &str,
    ) -> Result<CloudInstance, String>;
    async fn start_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String>;
    async fn stop_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String>;
    async fn reboot_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String>;
    async fn terminate_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String>;
    async fn create_instance(
        &self,
        creds: &CloudCredentials,
        name: &str,
        image_id: &str,
        flavor: &str,
        system_disk_gb: u32,
    ) -> Result<CloudInstance, String>;
    async fn list_networks(&self, creds: &CloudCredentials) -> Result<Vec<CloudVpc>, String>;
    async fn list_security_groups(
        &self,
        creds: &CloudCredentials,
    ) -> Result<Vec<CloudSecurityGroup>, String>;
    async fn list_regions(&self, creds: &CloudCredentials) -> Result<Vec<CloudRegion>, String>;
}

/// Resolve persisted credentials for all cloud automation operations.
pub async fn resolve_creds_inner(
    state: &ScanState,
    config_id: i32,
) -> Result<CloudCredentials, AppError> {
    let cfg: (String, String, String, String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT provider,region_id,region_name,account_name,access_key_id,access_key_secret,auth_type,auth_username,auth_password,auth_domain,project_id
         FROM scan_cloud_provider_configs
         WHERE id=$1 AND deleted=0 AND status='active'",
    )
    .bind(config_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("active cloud provider config not found"))?;

    let kind = match cfg.6.as_str() {
        "application_credential" | "keystone_password" => CloudPlatformKind::OpenStack,
        "iam_password" => CloudPlatformKind::Hcs,
        _ => match cfg.0.as_str() {
            "aliyun" | "apsara" => CloudPlatformKind::Apsara,
            "huawei" | "hcs" => CloudPlatformKind::Hcs,
            "tencent" | "tstack" => CloudPlatformKind::TStack,
            "openstack" => CloudPlatformKind::OpenStack,
            other => CloudPlatformKind::Custom(other.to_string()),
        },
    };

    let uses_account_token = matches!(cfg.6.as_str(), "iam_password" | "keystone_password");
    Ok(CloudCredentials {
        kind,
        endpoint: cfg.1,
        username: uses_account_token.then(|| cfg.7.unwrap_or(cfg.3)),
        password: uses_account_token.then(|| cfg.8).flatten(),
        domain: uses_account_token.then(|| cfg.9).flatten(),
        project_name: uses_account_token.then(|| cfg.10.clone()).flatten(),
        project_id: uses_account_token.then(|| cfg.10).flatten(),
        access_key: (!uses_account_token).then_some(cfg.4),
        secret_key: (!uses_account_token).then_some(cfg.5),
        region: Some(cfg.2),
        token: None,
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────

pub async fn test_connection(
    State(state): State<ScanState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<CloudTestResult>>, AppError> {
    let config_id = req["config_id"].as_i64().unwrap_or(0) as i32;
    let creds = resolve_creds_inner(&state, config_id).await?;
    let adapter = get_adapter_inner(&creds.kind);
    let result = adapter
        .test_connection(&creds)
        .await
        .unwrap_or_else(|e| CloudTestResult {
            success: false,
            message: e,
            response_time_ms: None,
            tested_at: chrono::Utc::now(),
        });
    Ok(Json(ApiResponse::new(result)))
}

pub async fn list_instances(
    State(state): State<ScanState>,
    Path(config_id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<CloudInstance>>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let list = get_adapter_inner(&creds.kind)
        .list_instances(&creds)
        .await
        .map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(list)))
}

pub async fn get_instance(
    State(state): State<ScanState>,
    Path((config_id, instance_id)): Path<(i32, String)>,
) -> Result<Json<ApiResponse<CloudInstance>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let inst = get_adapter_inner(&creds.kind)
        .get_instance(&creds, &instance_id)
        .await
        .map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(inst)))
}

pub async fn instance_action(
    State(state): State<ScanState>,
    Path((config_id, instance_id)): Path<(i32, String)>,
    Json(req): Json<CloudInstanceActionRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let adapter = get_adapter_inner(&creds.kind);
    let result = match req.action {
        kairos_platform_scan_api::CloudInstanceAction::Start => {
            adapter.start_instance(&creds, &instance_id).await
        }
        kairos_platform_scan_api::CloudInstanceAction::Stop => {
            adapter.stop_instance(&creds, &instance_id).await
        }
        kairos_platform_scan_api::CloudInstanceAction::Reboot => {
            adapter.reboot_instance(&creds, &instance_id).await
        }
        kairos_platform_scan_api::CloudInstanceAction::Terminate => {
            adapter.terminate_instance(&creds, &instance_id).await
        }
    };
    result.map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(json!({"message": "操作成功"}))))
}

pub async fn create_instance(
    State(state): State<ScanState>,
    Path(config_id): Path<i32>,
    Json(req): Json<CloudCreateInstanceRequest>,
) -> Result<Json<ApiResponse<CloudInstance>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let inst = get_adapter_inner(&creds.kind)
        .create_instance(
            &creds,
            &req.name,
            &req.image_id,
            &req.flavor,
            req.system_disk_gb,
        )
        .await
        .map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(inst)))
}

pub async fn list_networks(
    State(state): State<ScanState>,
    Path(config_id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<CloudVpc>>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let list = get_adapter_inner(&creds.kind)
        .list_networks(&creds)
        .await
        .map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(list)))
}

pub async fn list_security_groups(
    State(state): State<ScanState>,
    Path(config_id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<CloudSecurityGroup>>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let list = get_adapter_inner(&creds.kind)
        .list_security_groups(&creds)
        .await
        .map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(list)))
}

pub async fn list_regions(
    State(state): State<ScanState>,
    Path(config_id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<CloudRegion>>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let list = get_adapter_inner(&creds.kind)
        .list_regions(&creds)
        .await
        .map_err(|e| AppError::internal(e))?;
    Ok(Json(ApiResponse::new(list)))
}

pub async fn sync(
    State(state): State<ScanState>,
    Path(config_id): Path<i32>,
    Json(_req): Json<CloudSyncRequest>,
) -> Result<Json<ApiResponse<CloudSyncResult>>, AppError> {
    let creds = resolve_creds_inner(&state, config_id).await?;
    let adapter = get_adapter_inner(&creds.kind);
    let start = std::time::Instant::now();
    let mut errors = Vec::new();
    let mut instances_synced = 0u32;
    let mut networks_synced = 0u32;

    match adapter.list_instances(&creds).await {
        Ok(instances) => {
            instances_synced = instances.len() as u32;
            let mut tx = state
                .pool
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
            let instance_ids: Vec<String> = instances
                .iter()
                .map(|instance| instance.instance_id.clone())
                .collect();
            for inst in &instances {
                let updated = sqlx::query("UPDATE scan_business_resources SET ecs_name=$3,ecs_status=$4,resource_id=$2,cloud_region=$5,cloud_category=$6,ecs_type=$7,ecs_os=$8,cpu_cores=$9,memory_gb=$10,system_disk_size_gb=$11,ip_address=$12,deleted=0,updated_at=now() WHERE cloud_provider_config_id=$1 AND instance_id=$2")
                    .bind(config_id).bind(&inst.instance_id).bind(&inst.name).bind(&inst.status)
                    .bind(creds.region.as_deref().unwrap_or_default()).bind(format!("{:?}", creds.kind))
                    .bind(&inst.flavor).bind(&inst.os_type).bind(inst.cpu_cores as i32)
                    .bind((inst.memory_mb / 1024) as i32).bind(inst.system_disk_gb as i32)
                    .bind(inst.private_ip.first().map(String::as_str).unwrap_or_default())
                    .execute(&mut *tx).await.map_err(|e| AppError::internal(e.to_string()))?;
                if updated.rows_affected() == 0 {
                    sqlx::query("INSERT INTO scan_business_resources(resource_type,ecs_name,ecs_status,resource_id,cloud_region,cloud_category,customer_name,instance_id,ecs_type,ecs_os,cpu_cores,memory_gb,system_disk,system_disk_size_gb,ip_address,application_status,delivery_status,remarks,cloud_provider_config_id) VALUES('cloud',$1,$2,$3,$4,$5,'云平台同步',$3,$6,$7,$8,$9,'cloud_ssd',$10,$11,'已同步','已交付','synced from cloud platform',$12)")
                        .bind(&inst.name).bind(&inst.status).bind(&inst.instance_id)
                        .bind(creds.region.as_deref().unwrap_or_default()).bind(format!("{:?}", creds.kind))
                        .bind(&inst.flavor).bind(&inst.os_type).bind(inst.cpu_cores as i32)
                        .bind((inst.memory_mb / 1024) as i32).bind(inst.system_disk_gb as i32)
                        .bind(inst.private_ip.first().map(String::as_str).unwrap_or_default()).bind(config_id)
                        .execute(&mut *tx).await.map_err(|e| AppError::internal(e.to_string()))?;
                }
            }
            sqlx::query("UPDATE scan_business_resources SET deleted=1,ecs_status='云端已删除',updated_at=now() WHERE cloud_provider_config_id=$1 AND deleted=0 AND NOT (instance_id = ANY($2))")
                .bind(config_id).bind(&instance_ids)
                .execute(&mut *tx).await.map_err(|e| AppError::internal(e.to_string()))?;
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }
        Err(e) => errors.push(format!("instances: {e}")),
    }
    match adapter.list_networks(&creds).await {
        Ok(networks) => networks_synced = networks.len() as u32,
        Err(e) => errors.push(format!("networks: {e}")),
    }

    Ok(Json(ApiResponse::new(CloudSyncResult {
        platform: creds.kind,
        instances_synced,
        networks_synced,
        errors,
        synced_at: chrono::Utc::now(),
    })))
}

pub fn get_adapter_inner(kind: &CloudPlatformKind) -> Box<dyn CloudApi> {
    match kind {
        CloudPlatformKind::OpenStack => Box::new(crate::cloud_openstack::OpenStackAdapter::new()),
        CloudPlatformKind::Hcs => Box::new(crate::cloud_hcs::HcsAdapter::new()),
        CloudPlatformKind::Apsara => Box::new(crate::cloud_apsara::ApsaraAdapter::new()),
        CloudPlatformKind::TStack => Box::new(crate::cloud_tstack::TStackAdapter::new()),
        CloudPlatformKind::Custom(_) => Box::new(crate::cloud_openstack::OpenStackAdapter::new()),
    }
}
