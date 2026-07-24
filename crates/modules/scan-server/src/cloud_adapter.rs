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

/// Resolve credentials from config store.
pub async fn resolve_creds_inner(
    state: &ScanState,
    config_id: i32,
) -> Result<CloudCredentials, AppError> {
    let configs = state.cloud_provider_configs.lock().await;
    let cfg = configs
        .iter()
        .find(|c| c.id == Some(config_id))
        .ok_or_else(|| AppError::not_found("cloud provider config not found"))?;

    let kind = match cfg.provider {
        kairos_platform_scan_api::CloudProviderVendor::Aliyun => CloudPlatformKind::Apsara,
        kairos_platform_scan_api::CloudProviderVendor::Huawei => CloudPlatformKind::Hcs,
        kairos_platform_scan_api::CloudProviderVendor::Tencent => CloudPlatformKind::TStack,
        ref other => {
            let s = format!("{:?}", other).to_lowercase();
            match s.as_str() {
                "openstack" => CloudPlatformKind::OpenStack,
                "hcs" => CloudPlatformKind::Hcs,
                "apsara" => CloudPlatformKind::Apsara,
                "tstack" => CloudPlatformKind::TStack,
                _ => CloudPlatformKind::Custom(s),
            }
        }
    };

    Ok(CloudCredentials {
        kind,
        endpoint: cfg.region_id.clone(),
        username: Some(cfg.account_name.clone()),
        password: None,
        domain: None,
        project_name: None,
        project_id: None,
        access_key: Some(cfg.access_key_id.clone()),
        secret_key: Some(cfg.access_key_secret.clone()),
        region: Some(cfg.region_name.clone()),
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
            // Store synced instances as cloud service assets
            let mut assets = state.business_resources.lock().await;
            for inst in &instances {
                use kairos_platform_scan_api::BusinessResource;
                let next_id = assets.len() as i32 + 1;
                assets.push(BusinessResource {
                    id: Some(next_id),
                    resource_type: "cloud".into(),
                    ecs_name: inst.name.clone(),
                    ecs_status: inst.status.clone(),
                    resource_id: inst.instance_id.clone(),
                    cloud_region: creds.region.clone().unwrap_or_default(),
                    cloud_category: format!("{:?}", creds.kind),
                    cloud_provider_config_id: Some(config_id),
                    zone_name: inst.zone.clone(),
                    platform_name: None,
                    county_city: None,
                    vdc_name: None,
                    customer_name: "sync".into(),
                    application_name: None,
                    contract_name: None,
                    instance_id: inst.instance_id.clone(),
                    ecs_type: inst.flavor.clone(),
                    ecs_os: inst.os_type.clone(),
                    cpu_cores: inst.cpu_cores,
                    memory_gb: inst.memory_mb / 1024,
                    system_disk: "cloud_ssd".into(),
                    system_disk_size_gb: inst.system_disk_gb,
                    data_disk: None,
                    completion_time: None,
                    release_time: None,
                    has_security_product: false,
                    ip_address: inst.private_ip.first().cloned().unwrap_or_default(),
                    ecs_login_method: None,
                    ecs_login_username: None,
                    ecs_initial_password: None,
                    bastion_address: None,
                    bastion_admin_account: None,
                    bastion_initial_password: None,
                    physical_machine_info: None,
                    cloud_vm_info: None,
                    remarks: Some("synced from cloud platform".into()),
                    created_at: Some(chrono::Utc::now()),
                    updated_at: Some(chrono::Utc::now()),
                    created_by: Some("cloud-sync".into()),
                    updated_by: Some("cloud-sync".into()),
                    applicant: None,
                    department: None,
                    approver: None,
                    approval_time: None,
                    approval_remarks: None,
                    rejection_reason: None,
                    bandwidth_mbps: None,
                    bandwidth_type: None,
                    public_ip_count: None,
                    network_type: None,
                    project_name: None,
                    project_code: None,
                    business_owner: None,
                    tech_owner: None,
                    contact_phone: None,
                    billing_method: None,
                    purchase_duration: None,
                    cost_center: None,
                    security_level: None,
                    data_sensitivity: None,
                    purpose: None,
                    expected_delivery_time: None,
                    application_status: Some("已同步".into()),
                    delivery_status: Some("已交付".into()),
                    delivery_confirmed_at: None,
                    delivery_confirmed_by: None,
                });
            }
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
