//! Scan server — ported from rustset business modules.
//!
//! Provides asset scanning, network zone management, port scanning,
//! scanner configuration, and scan task management.

mod asset;
mod asset_inventory;
mod business_application;
mod business_resource;
mod cabinet;
mod cloud_adapter;
mod cloud_apsara;
mod cloud_hcs;
mod cloud_openstack;
mod cloud_platform;
mod cloud_provider_config;
mod cloud_provisioning;
mod cloud_service_asset;
mod cloud_tstack;
mod dashboard;
mod infrastructure_device;
mod machine_room;
mod physical_resource;
mod port_detail;
mod resource_ticket;
mod resource_ticket_approval;
mod resource_ticket_validation;
mod risk;
mod scanner;
mod security_product;
mod service_provider;
mod task;
mod vulnerability_rule;
mod zone;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::Mutex;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use ipnetwork::IpNetwork;
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{
    Asset, BusinessResource, CloudPlatform, CloudProviderConfig, CloudSecurityGroup, CloudZone,
    CreateScannerRequest, CreateTaskRequest, MachineRoom, PortDetail, PortInfo, QuickScanResult,
    RegisterScannerAgentRequest, ResourceTicket, Risk, ScanCapability, ScanRequest, ScanTask,
    ScannerConfig, ScannerHeartbeatRequest, ScannerTargetResult, ScannerTaskClaim,
    ScannerTaskClaimRequest, ScannerTaskCompleteRequest, SecurityProduct, ServiceProvider,
    UpdateScannerRequest, ZoneConfig,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ── App state ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ScanState {
    pool: PgPool,
    zones: Arc<Mutex<Vec<ZoneConfig>>>,
    assets: Arc<Mutex<Vec<Asset>>>,
    tasks: Arc<Mutex<Vec<ScanTask>>>,
    scanners: Arc<Mutex<Vec<ScannerConfig>>>,
    scan_results: Arc<Mutex<Vec<QuickScanResult>>>,
    port_details: Arc<Mutex<Vec<PortDetail>>>,
    machine_rooms: Arc<Mutex<Vec<MachineRoom>>>,
    service_providers: Arc<Mutex<Vec<ServiceProvider>>>,
    security_products: Arc<Mutex<Vec<SecurityProduct>>>,
    cloud_zones: Arc<Mutex<Vec<CloudZone>>>,
    cloud_platforms: Arc<Mutex<Vec<CloudPlatform>>>,
    resource_tickets: Arc<Mutex<Vec<ResourceTicket>>>,
    business_resources: Arc<Mutex<Vec<BusinessResource>>>,
    cloud_provider_configs: Arc<Mutex<Vec<CloudProviderConfig>>>,
    risks: Arc<Mutex<Vec<Risk>>>,
    cloud_firewalls: Arc<Mutex<Vec<CloudSecurityGroup>>>,
}

impl ScanState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            zones: Arc::new(Mutex::new(Self::default_zones())),
            assets: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            scanners: Arc::new(Mutex::new(Vec::new())),
            scan_results: Arc::new(Mutex::new(Vec::new())),
            port_details: Arc::new(Mutex::new(Vec::new())),
            machine_rooms: Arc::new(Mutex::new(Vec::new())),
            service_providers: Arc::new(Mutex::new(Vec::new())),
            security_products: Arc::new(Mutex::new(Vec::new())),
            cloud_zones: Arc::new(Mutex::new(Vec::new())),
            cloud_platforms: Arc::new(Mutex::new(Vec::new())),
            resource_tickets: Arc::new(Mutex::new(Vec::new())),
            business_resources: Arc::new(Mutex::new(Vec::new())),
            cloud_provider_configs: Arc::new(Mutex::new(Vec::new())),
            risks: Arc::new(Mutex::new(Vec::new())),
            cloud_firewalls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn default_zones() -> Vec<ZoneConfig> {
        vec![
            ZoneConfig {
                id: "1".into(),
                name: "Intranet".into(),
                cidr: "192.168.0.0/16".into(),
                priority: 10,
                cloud_platform_id: None,
                cloud_platform_name: None,
                machine_room_id: None,
                machine_room_name: None,
            },
            ZoneConfig {
                id: "2".into(),
                name: "DMZ".into(),
                cidr: "10.0.0.0/8".into(),
                priority: 20,
                cloud_platform_id: None,
                cloud_platform_name: None,
                machine_room_id: None,
                machine_room_name: None,
            },
        ]
    }
}

// ── Routes ────────────────────────────────────────────────────────────────

pub fn routes(state: ScanState) -> Router {
    Router::new()
        .route("/asset-ops/capabilities", get(capabilities))
        // Scanner configs
        .route(
            "/asset-ops/scanners",
            get(scanner_list).post(scanner_create),
        )
        .route(
            "/asset-ops/scanners/{id}",
            put(scanner_update).delete(scanner_delete),
        )
        .route(
            "/asset-ops/scanner-agents/register",
            post(scanner_agent_register),
        )
        .route(
            "/asset-ops/scanner-agents/heartbeat",
            post(scanner_agent_heartbeat),
        )
        .route(
            "/asset-ops/scanner-agents/tasks/claim",
            post(scanner_agent_task_claim),
        )
        .route(
            "/asset-ops/scanner-agents/tasks/complete",
            post(scanner_agent_task_complete),
        )
        // Zones
        .route("/asset-ops/zones", get(zone_list).post(zone_create))
        .route(
            "/asset-ops/zones/{id}",
            get(zone_get).put(zone_update).delete(zone_delete),
        )
        // Assets
        .route("/asset-ops/assets", get(asset_list).post(asset_create))
        .route(
            "/asset-ops/assets/{id}",
            get(asset_get).put(asset_update).delete(asset_delete),
        )
        // Tasks
        .route("/asset-ops/tasks", get(task_list).post(task_create))
        .route("/asset-ops/tasks/{id}", get(task_get).delete(task_delete))
        .route("/asset-ops/tasks/{id}/execute", post(task_execute))
        // Scan
        .route("/asset-ops/scan", post(scan_execute))
        .route("/asset-ops/results", get(scan_results_list))
        .route("/asset-ops/results/{id}", get(scan_result_get))
        .route("/asset-ops/check-port", post(check_single_port))
        // Port details
        .route("/asset-ops/port-details", get(port_detail::list))
        .route(
            "/asset-ops/port-details/{id}",
            get(port_detail::get_one).put(port_detail::update_one),
        )
        // Machine rooms
        .route(
            "/asset-ops/machine-rooms",
            get(machine_room::list).post(machine_room::create),
        )
        .route(
            "/asset-ops/machine-rooms/{id}",
            get(machine_room::get_one)
                .put(machine_room::update)
                .delete(machine_room::delete_one),
        )
        // Service providers
        .route(
            "/asset-ops/cabinet-rows",
            get(cabinet::row_list).post(cabinet::row_create),
        )
        .route(
            "/asset-ops/cabinet-rows/{id}",
            put(cabinet::row_update).delete(cabinet::row_delete),
        )
        .route(
            "/asset-ops/cabinets",
            get(cabinet::list).post(cabinet::create),
        )
        .route(
            "/asset-ops/cabinets/configure-room",
            post(cabinet::configure_room),
        )
        .route(
            "/asset-ops/cabinets/{id}",
            put(cabinet::update).delete(cabinet::delete_one),
        )
        .route(
            "/asset-ops/service-providers",
            get(service_provider::list).post(service_provider::create),
        )
        .route(
            "/asset-ops/service-providers/{id}",
            get(service_provider::get_one)
                .put(service_provider::update)
                .delete(service_provider::delete_one),
        )
        // Security products
        .route(
            "/asset-ops/security-products",
            get(security_product::list).post(security_product::create),
        )
        .route(
            "/asset-ops/security-products/{id}",
            get(security_product::get_one)
                .put(security_product::update)
                .delete(security_product::delete_one),
        )
        // Network and storage infrastructure devices
        .route(
            "/asset-ops/infrastructure-devices",
            get(infrastructure_device::list).post(infrastructure_device::create),
        )
        .route(
            "/asset-ops/infrastructure-devices/{id}",
            put(infrastructure_device::update).delete(infrastructure_device::delete_one),
        )
        // Cloud zones & platforms
        .route(
            "/asset-ops/cloud-zones",
            get(cloud_platform::zone_list).post(cloud_platform::zone_create),
        )
        .route(
            "/asset-ops/cloud-zones/{id}",
            get(cloud_platform::zone_get)
                .put(cloud_platform::zone_update)
                .delete(cloud_platform::zone_delete),
        )
        .route(
            "/asset-ops/cloud-platforms",
            get(cloud_platform::platform_list).post(cloud_platform::platform_create),
        )
        .route(
            "/asset-ops/cloud-platforms/{id}",
            get(cloud_platform::platform_get)
                .put(cloud_platform::platform_update)
                .delete(cloud_platform::platform_delete),
        )
        // Resource tickets
        .route(
            "/asset-ops/resource-tickets",
            get(resource_ticket::list).post(resource_ticket::create),
        )
        .route(
            "/asset-ops/resource-tickets/{id}",
            get(resource_ticket::get_one)
                .put(resource_ticket::update)
                .delete(resource_ticket::delete_one),
        )
        .route(
            "/asset-ops/resource-tickets/{id}/approve",
            post(resource_ticket::approve),
        )
        .route(
            "/asset-ops/resource-tickets/{id}/approvals",
            get(resource_ticket::approval_history),
        )
        .route(
            "/asset-ops/resource-tickets/{id}/provision",
            post(resource_ticket::provision),
        )
        .route(
            "/asset-ops/resource-tickets/{id}/deliver",
            post(resource_ticket::deliver),
        )
        // Dashboard
        .route("/asset-ops/dashboard", get(dashboard::summary))
        // Business resources
        .route(
            "/asset-ops/business-resources",
            get(business_resource::list).post(business_resource::create),
        )
        .route(
            "/asset-ops/business-resources/{id}",
            get(business_resource::get_one)
                .put(business_resource::update)
                .delete(business_resource::delete_one),
        )
        // Business applications
        .route(
            "/asset-ops/business-applications",
            get(business_application::list).post(business_application::create),
        )
        .route(
            "/asset-ops/business-applications/{id}",
            get(business_application::get_one)
                .put(business_application::update)
                .delete(business_application::delete_one),
        )
        .route(
            "/asset-ops/business-applications/{id}/endpoints",
            get(business_application::endpoints).post(business_application::add_endpoint),
        )
        // Cloud provider configs
        .route(
            "/asset-ops/cloud-provider-configs",
            get(cloud_provider_config::list).post(cloud_provider_config::create),
        )
        .route(
            "/asset-ops/cloud-provider-configs/{id}",
            get(cloud_provider_config::get_one)
                .put(cloud_provider_config::update)
                .delete(cloud_provider_config::delete_one),
        )
        // Cloud service assets
        .route(
            "/asset-ops/cloud-service-assets",
            get(cloud_service_asset::list),
        )
        .route(
            "/asset-ops/cloud-service-assets/{id}",
            get(cloud_service_asset::get_one),
        )
        // Risks
        .route("/asset-ops/risks", get(risk::list))
        .route(
            "/asset-ops/risks/{id}",
            get(risk::get_one).put(risk::update_one),
        )
        .route(
            "/asset-ops/vulnerability-rules",
            get(vulnerability_rule::list),
        )
        .route(
            "/asset-ops/vulnerability-rules/{id}",
            post(vulnerability_rule::create)
                .put(vulnerability_rule::update)
                .delete(vulnerability_rule::delete_one),
        )
        // Cloud SDK operations
        .route(
            "/asset-ops/cloud/test",
            post(cloud_adapter::test_connection),
        )
        .route(
            "/asset-ops/cloud/{config_id}/instances",
            get(cloud_adapter::list_instances).post(cloud_adapter::create_instance),
        )
        .route(
            "/asset-ops/cloud/{config_id}/instances/{instance_id}",
            get(cloud_adapter::get_instance),
        )
        .route(
            "/asset-ops/cloud/{config_id}/instances/{instance_id}/action",
            post(cloud_adapter::instance_action),
        )
        .route(
            "/asset-ops/cloud/{config_id}/networks",
            get(cloud_adapter::list_networks),
        )
        .route(
            "/asset-ops/cloud/{config_id}/security-groups",
            get(cloud_adapter::list_security_groups),
        )
        .route(
            "/asset-ops/cloud/{config_id}/regions",
            get(cloud_adapter::list_regions),
        )
        .route(
            "/asset-ops/cloud/{config_id}/sync",
            post(cloud_adapter::sync),
        )
        // Cloud-specific endpoints
        .route(
            "/asset-ops/cloud/openstack/{config_id}/projects",
            get(cloud_openstack::list_projects),
        )
        .route(
            "/asset-ops/cloud/openstack/{config_id}/flavors",
            get(cloud_openstack::list_flavors),
        )
        .route(
            "/asset-ops/cloud/hcs/{config_id}/projects",
            get(cloud_hcs::list_projects),
        )
        .route(
            "/asset-ops/cloud/apsara/{config_id}/zones",
            get(cloud_apsara::list_zones),
        )
        .route(
            "/asset-ops/cloud/tstack/{config_id}/clusters",
            get(cloud_tstack::list_clusters),
        )
        // Cloud firewall sync
        // Health
        .route("/asset-ops/health", get(health))
        .with_state(state)
}

async fn capabilities() -> Json<ApiResponse<ScanCapability>> {
    Json(ApiResponse::new(ScanCapability::default()))
}

async fn health(State(s): State<ScanState>) -> Json<ApiResponse<Value>> {
    let zones =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scan_cloud_zones WHERE deleted=0")
            .fetch_one(&s.pool)
            .await
            .unwrap_or(0);
    let assets = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scan_assets WHERE deleted=0")
        .fetch_one(&s.pool)
        .await
        .unwrap_or(0);
    let tasks = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scan_tasks WHERE deleted=0")
        .fetch_one(&s.pool)
        .await
        .unwrap_or(0);
    Json(ApiResponse::new(
        json!({"status":"ok","zones":zones,"assets":assets,"tasks":tasks}),
    ))
}

// ── Scanner CRUD ──────────────────────────────────────────────────────────

async fn scanner_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<ScannerConfig>>>, AppError> {
    sqlx::query("UPDATE scan_scanners SET agent_status='offline' WHERE agent_status='online' AND last_heartbeat < now()-interval '90 seconds'")
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_scanners t WHERE deleted=0 ORDER BY id",
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
async fn scanner_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateScannerRequest>,
) -> Result<Json<ApiResponse<ScannerConfig>>, AppError> {
    let id = Uuid::new_v4().to_string();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let now = chrono::Utc::now().to_rfc3339();
    let capabilities = json!([
        "host_discovery",
        "port_scan",
        "service_detection",
        "site_identify"
    ]);
    sqlx::query("INSERT INTO scan_scanners(id,name,scanner_type,enabled,config,registration_token_hash,token_expires_at,agent_status,capabilities,created_at,updated_at) VALUES($1,$2,'rustscan',$3,$4,$5,now()+interval '24 hours','pending',$6,now(),now())")
        .bind(&id).bind(&req.name).bind(req.enabled.unwrap_or(true)).bind(&req.config).bind(&token_hash).bind(capabilities)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(ScannerConfig {
        id,
        name: req.name,
        scanner_type: "rustscan".into(),
        enabled: req.enabled.unwrap_or(true),
        config: req.config,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        agent_status: Some("pending".into()),
        agent_version: None,
        agent_hostname: None,
        agent_ip: None,
        capabilities: vec![
            "host_discovery".into(),
            "port_scan".into(),
            "service_detection".into(),
            "site_identify".into(),
        ],
        registered_at: None,
        last_heartbeat: None,
        registration_token: Some(token),
    })))
}

async fn scanner_agent_register(
    State(s): State<ScanState>,
    Json(req): Json<RegisterScannerAgentRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let token_hash = format!("{:x}", Sha256::digest(req.token.as_bytes()));
    let capabilities = if req.capabilities.is_empty() {
        json!([
            "host_discovery",
            "port_scan",
            "service_detection",
            "site_identify"
        ])
    } else {
        json!(req.capabilities)
    };
    let id = sqlx::query_scalar::<_, String>("UPDATE scan_scanners SET agent_status='online',agent_version=$2,agent_hostname=$3,agent_ip=$4,capabilities=$5,registered_at=COALESCE(registered_at,now()),last_heartbeat=now(),updated_at=now() WHERE registration_token_hash=$1 AND deleted=0 AND enabled=true AND (registered_at IS NOT NULL OR token_expires_at>now()) RETURNING id")
        .bind(token_hash).bind(req.agent_version).bind(req.hostname).bind(req.ip_address).bind(capabilities)
        .fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::bad_request("注册令牌无效、已过期或节点已禁用"))?;
    Ok(Json(ApiResponse::new(
        json!({"node_id":id,"heartbeat_interval_seconds":30}),
    )))
}

async fn scanner_agent_heartbeat(
    State(s): State<ScanState>,
    Json(req): Json<ScannerHeartbeatRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let token_hash = format!("{:x}", Sha256::digest(req.token.as_bytes()));
    let updated = sqlx::query("UPDATE scan_scanners SET agent_status='online',last_heartbeat=now(),updated_at=now() WHERE id=$1 AND registration_token_hash=$2 AND registered_at IS NOT NULL AND enabled=true AND deleted=0")
        .bind(&req.node_id).bind(token_hash).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    if updated.rows_affected() == 0 {
        return Err(AppError::bad_request("节点凭证无效或节点已禁用"));
    }
    if let (Some(task_id), Some(claim_token)) = (req.task_id, req.claim_token) {
        sqlx::query(
            "UPDATE scan_tasks SET lease_expires_at=now()+interval '15 minutes'
             WHERE id=$1 AND scanner_id=$2 AND claim_token=$3 AND status='running'",
        )
        .bind(task_id)
        .bind(req.node_id)
        .bind(claim_token)
        .execute(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    }
    Ok(Json(ApiResponse::new(json!({"status":"online"}))))
}

async fn authenticate_scanner_agent(
    pool: &PgPool,
    node_id: &str,
    token: &str,
) -> Result<(), AppError> {
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM scan_scanners
            WHERE id=$1 AND registration_token_hash=$2 AND registered_at IS NOT NULL
              AND enabled=true AND deleted=0
        )",
    )
    .bind(node_id)
    .bind(token_hash)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if !valid {
        return Err(AppError::bad_request("节点凭证无效或节点已禁用"));
    }
    Ok(())
}

fn ports_for_policy(policy: &str) -> Vec<u16> {
    match policy {
        "TOP100" => scanner::common_ports::TOP100.to_vec(),
        "TOP1000" => scanner::common_ports::TOP1000.to_vec(),
        _ => vec![
            22, 80, 443, 445, 3306, 3389, 5432, 6379, 8000, 8008, 8080, 8081, 8443, 8888, 27017,
        ],
    }
}

async fn scanner_agent_task_claim(
    State(s): State<ScanState>,
    Json(req): Json<ScannerTaskClaimRequest>,
) -> Result<Json<ApiResponse<Option<ScannerTaskClaim>>>, AppError> {
    authenticate_scanner_agent(&s.pool, &req.node_id, &req.token).await?;
    let mut transaction = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;

    sqlx::query(
        "UPDATE scan_tasks
         SET status='pending', claim_token=NULL, lease_expires_at=NULL,
             error_message='上一次执行租约已过期，任务已重新排队'
         WHERE scanner_id=$1 AND status='running' AND lease_expires_at < now()",
    )
    .bind(&req.node_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let value = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_tasks t
         WHERE scanner_id=$1 AND status='pending' AND deleted=0
         ORDER BY created_at, id
         FOR UPDATE SKIP LOCKED
         LIMIT 1",
    )
    .bind(&req.node_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

    let Some(value) = value else {
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
        return Ok(Json(ApiResponse::new(None)));
    };
    let task: ScanTask =
        serde_json::from_value(value).map_err(|error| AppError::internal(error.to_string()))?;
    let claim_token = Uuid::new_v4().simple().to_string();
    let lease_seconds = 900_u64;
    let updated = sqlx::query(
        "UPDATE scan_tasks
         SET status='running', start_time=COALESCE(start_time, now()),
             claim_token=$2, lease_expires_at=now()+make_interval(secs => $3),
             attempt_count=attempt_count+1, error_message=NULL
         WHERE id=$1 AND status='pending'",
    )
    .bind(&task.id)
    .bind(&claim_token)
    .bind(lease_seconds as f64)
    .execute(&mut *transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if updated.rows_affected() != 1 {
        return Err(AppError::internal("扫描任务领取竞争失败"));
    }
    transaction
        .commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;

    Ok(Json(ApiResponse::new(Some(ScannerTaskClaim {
        ports: ports_for_policy(&task.port_policy),
        task,
        claim_token,
        lease_seconds,
    }))))
}

async fn scanner_agent_task_complete(
    State(s): State<ScanState>,
    Json(req): Json<ScannerTaskCompleteRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    authenticate_scanner_agent(&s.pool, &req.node_id, &req.token).await?;
    let mut transaction = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let (task_target, port_policy) = sqlx::query_as::<_, (String, String)>(
        "SELECT target,port_policy FROM scan_tasks
         WHERE id=$1 AND scanner_id=$2 AND status='running' AND claim_token=$3
           AND lease_expires_at >= now()
         FOR UPDATE",
    )
    .bind(&req.task_id)
    .bind(&req.node_id)
    .bind(&req.claim_token)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::bad_request("任务租约无效或已过期"))?;

    if let Some(error_message) = req
        .error_message
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        sqlx::query(
            "UPDATE scan_tasks SET status='failed', end_time=now(), error_message=$2,
                 claim_token=NULL, lease_expires_at=NULL WHERE id=$1",
        )
        .bind(&req.task_id)
        .bind(error_message.chars().take(2000).collect::<String>())
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
        return Ok(Json(ApiResponse::new(json!({"status":"failed"}))));
    }

    let allowed_targets = task::parse_targets(&task_target);
    if req.results.len() > 4096
        || req.results.iter().any(|result| {
            !target_is_allowed(&result.target, &allowed_targets) || !scan_result_is_valid(result)
        })
    {
        return Err(AppError::bad_request(
            "扫描结果包含任务范围外的目标或无效端口列表",
        ));
    }

    let scanned_ports = ports_for_policy(&port_policy);
    let (found_assets, found_risks) =
        persist_agent_results(&mut transaction, &req.results, &scanned_ports).await?;
    sqlx::query(
        "UPDATE scan_tasks SET status='completed', end_time=now(), found_assets=$2,
             found_risks=$3, claim_token=NULL, lease_expires_at=NULL, error_message=NULL
         WHERE id=$1",
    )
    .bind(&req.task_id)
    .bind(found_assets as i32)
    .bind(found_risks as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    transaction
        .commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({
        "status": "completed",
        "assets_found": found_assets,
        "risks_found": found_risks,
    }))))
}

fn scan_result_is_valid(result: &ScannerTargetResult) -> bool {
    if result.open_ports.len() > 65_535
        || result.ports.len() > 65_535
        || result
            .os_hint
            .as_ref()
            .is_some_and(|value| value.len() > 500)
        || result
            .fingerprint
            .as_ref()
            .is_some_and(|value| value.len() > 1000)
    {
        return false;
    }
    result.ports.iter().all(|port| {
        (result.open_ports.is_empty() || result.open_ports.contains(&port.port))
            && port.service.as_ref().is_none_or(|value| value.len() <= 100)
            && port.version.as_ref().is_none_or(|value| value.len() <= 200)
            && port.banner.as_ref().is_none_or(|value| value.len() <= 2000)
            && port
                .fingerprint
                .as_ref()
                .is_none_or(|value| value.to_string().len() <= 4096)
    })
}

fn target_is_allowed(target: &str, declared_targets: &[String]) -> bool {
    if declared_targets.iter().any(|declared| declared == target) {
        return true;
    }
    let Ok(address) = target.parse() else {
        return false;
    };
    declared_targets.iter().any(|declared| {
        declared
            .parse::<IpNetwork>()
            .is_ok_and(|network| network.contains(address))
    })
}

#[derive(Debug, Clone, PartialEq)]
struct DetectedRisk {
    rule_id: String,
    port: u16,
    severity: String,
    description: String,
    solution: String,
    initial_status: String,
    evidence: Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct VulnerabilityCatalogRule {
    id: String,
    cve_id: String,
    product: String,
    match_type: String,
    exact_version: Option<String>,
    cpe_uri: Option<String>,
    version_start_including: Option<String>,
    version_start_excluding: Option<String>,
    version_end_including: Option<String>,
    version_end_excluding: Option<String>,
    severity: String,
    title: String,
    description: String,
    solution: String,
    reference_url: String,
    default_status: String,
}

fn version_parts(value: &str) -> Vec<(bool, String)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut numeric = None;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                parts.push((numeric.unwrap_or(false), std::mem::take(&mut current)));
            }
            numeric = None;
            continue;
        }
        let is_numeric = character.is_ascii_digit();
        if numeric.is_some_and(|previous| previous != is_numeric) {
            parts.push((numeric.unwrap_or(false), std::mem::take(&mut current)));
        }
        numeric = Some(is_numeric);
        current.push(character.to_ascii_lowercase());
    }
    if !current.is_empty() {
        parts.push((numeric.unwrap_or(false), current));
    }
    parts
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let left = version_parts(left);
    let right = version_parts(right);
    for index in 0..left.len().max(right.len()) {
        let left_part = left.get(index);
        let right_part = right.get(index);
        let ordering = match (left_part, right_part) {
            (Some((true, left)), Some((true, right))) => {
                let left = left.trim_start_matches('0');
                let right = right.trim_start_matches('0');
                left.len().cmp(&right.len()).then_with(|| left.cmp(right))
            }
            (Some((left_numeric, left)), Some((right_numeric, right))) => left_numeric
                .cmp(right_numeric)
                .then_with(|| left.cmp(right)),
            (Some((numeric, value)), None) => {
                if *numeric && value.bytes().all(|byte| byte == b'0') {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                }
            }
            (None, Some((numeric, value))) => {
                if *numeric && value.bytes().all(|byte| byte == b'0') {
                    Ordering::Equal
                } else {
                    Ordering::Less
                }
            }
            (None, None) => Ordering::Equal,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn cpe_product(cpe_uri: &str) -> Option<&str> {
    let mut fields = cpe_uri.strip_prefix("cpe:2.3:")?.split(':');
    let part = fields.next()?;
    let _vendor = fields.next()?;
    let product = fields.next()?;
    matches!(part, "a" | "o" | "h").then_some(product)
}

fn catalog_rule_matches(rule: &VulnerabilityCatalogRule, product: &str, version: &str) -> bool {
    let expected_product = rule
        .cpe_uri
        .as_deref()
        .and_then(cpe_product)
        .unwrap_or(&rule.product);
    if !product.contains(&expected_product.to_ascii_lowercase()) {
        return false;
    }
    if rule.match_type == "exact" || rule.exact_version.is_some() {
        if rule.exact_version.as_deref() != Some(version) {
            return false;
        }
    }
    let compare = |boundary: &Option<String>| {
        boundary
            .as_deref()
            .map(|boundary| compare_versions(version, boundary))
    };
    !matches!(
        compare(&rule.version_start_including),
        Some(std::cmp::Ordering::Less)
    ) && !matches!(
        compare(&rule.version_start_excluding),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    ) && !matches!(
        compare(&rule.version_end_including),
        Some(std::cmp::Ordering::Greater)
    ) && !matches!(
        compare(&rule.version_end_excluding),
        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
    )
}

fn extract_observed_version(value: &str) -> Option<&str> {
    value
        .split(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+'))
        })
        .find(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_digit)
                && part.contains('.')
                && part.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+')
                })
        })
}

fn detect_catalog_risks(
    ports: &[kairos_platform_scan_api::ScannerPortResult],
    catalog: &[VulnerabilityCatalogRule],
) -> Vec<DetectedRisk> {
    let mut risks = Vec::new();
    for port in ports {
        let service = port.service.as_deref().unwrap_or_default();
        let banner = port.banner.as_deref().unwrap_or_default();
        let version_text = port.version.as_deref().unwrap_or_default();
        let product_evidence = format!("{service} {version_text} {banner}").to_lowercase();
        let Some(observed_version) =
            extract_observed_version(version_text).or_else(|| extract_observed_version(banner))
        else {
            continue;
        };
        for rule in catalog {
            if !catalog_rule_matches(rule, &product_evidence, observed_version) {
                continue;
            }
            risks.push(DetectedRisk {
                rule_id: rule.id.clone(),
                port: port.port,
                severity: rule.severity.clone(),
                description: format!("{}：{}", rule.title, rule.description),
                solution: rule.solution.clone(),
                initial_status: rule.default_status.clone(),
                evidence: json!({
                    "match_type": rule.match_type,
                    "confidence": "version_only",
                    "cve_id": rule.cve_id,
                    "product": rule.product,
                    "observed_version": observed_version,
                    "matched_version": rule.exact_version,
                    "cpe_uri": rule.cpe_uri,
                    "version_range": {
                        "start_including": rule.version_start_including,
                        "start_excluding": rule.version_start_excluding,
                        "end_including": rule.version_end_including,
                        "end_excluding": rule.version_end_excluding
                    },
                    "service": service,
                    "banner": banner,
                    "reference_url": rule.reference_url,
                    "review_required": true
                }),
            });
        }
    }
    risks
}

fn detect_agent_risks(ports: &[kairos_platform_scan_api::ScannerPortResult]) -> Vec<DetectedRisk> {
    let mut risks = Vec::new();
    for port in ports {
        let service = port.service.as_deref().unwrap_or_default();
        let banner = port.banner.as_deref().unwrap_or_default();
        if service.eq_ignore_ascii_case("Redis") && banner.starts_with("+PONG") {
            risks.push(DetectedRisk {
                rule_id: "redis-unauthenticated-ping".to_string(),
                port: port.port,
                severity: "Medium".to_string(),
                description: format!(
                    "Redis {} 端口允许未认证客户端执行 PING，存在匿名访问面。",
                    port.port
                ),
                solution: "启用 Redis ACL/认证，绑定受控网络，并通过防火墙限制非必要来源。"
                    .to_string(),
                initial_status: "open".to_string(),
                evidence: json!({
                    "service": service,
                    "banner": banner,
                    "observation": "unauthenticated PING returned PONG"
                }),
            });
        }
        if service.eq_ignore_ascii_case("FTP") && banner.to_lowercase().contains("ftp") {
            risks.push(DetectedRisk {
                rule_id: "plaintext-ftp-service".to_string(),
                port: port.port,
                severity: "Low".to_string(),
                description: format!(
                    "检测到可直接建立明文控制连接的 FTP 服务（端口 {}）。",
                    port.port
                ),
                solution:
                    "优先迁移到 SFTP/FTPS；如必须保留 FTP，应限制来源并禁止明文传输敏感凭据。"
                        .to_string(),
                initial_status: "open".to_string(),
                evidence: json!({"service": service, "banner": banner}),
            });
        }
        if let Some(tls) = port.fingerprint.as_ref().and_then(|value| value.get("tls"))
            && tls.get("valid_now").and_then(Value::as_bool) == Some(false)
        {
            risks.push(DetectedRisk {
                rule_id: "tls-certificate-time-invalid".to_string(),
                port: port.port,
                severity: "Medium".to_string(),
                description: format!(
                    "TLS 证书在扫描时不处于有效期内（端口 {}），可能已过期或尚未生效。",
                    port.port
                ),
                solution: "核对系统时间和证书有效期，及时签发并部署当前有效的证书。".to_string(),
                initial_status: "open".to_string(),
                evidence: json!({"service": service, "tls": tls}),
            });
        }
    }
    risks
}

async fn sync_agent_risks(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target: &str,
    scanned_ports: &[u16],
    detected: &[DetectedRisk],
) -> Result<(), AppError> {
    let detected_keys = detected
        .iter()
        .map(|risk| (risk.port as i32, risk.rule_id.as_str()))
        .collect::<HashSet<_>>();
    let scanned_ports = scanned_ports
        .iter()
        .map(|port| *port as i32)
        .collect::<Vec<_>>();
    let existing = sqlx::query_as::<_, (String, i32, String)>(
        "SELECT id,port,rule_id FROM scan_risks
         WHERE asset_ip=$1 AND source='scan_agent' AND deleted=0
           AND port=ANY($2) AND rule_id IS NOT NULL
           AND status IN ('open','verified','pending_review')",
    )
    .bind(target)
    .bind(&scanned_ports)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    for (id, port, rule_id) in existing {
        if !detected_keys.contains(&(port, rule_id.as_str())) {
            sqlx::query(
                "UPDATE scan_risks SET status='resolved',updated_at=now()
                 WHERE id=$1",
            )
            .bind(id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
        }
    }
    for risk in detected {
        sqlx::query(
            "INSERT INTO scan_risks(
                 id,rule_id,asset_ip,port,severity,description,solution,status,evidence,
                 source,first_seen,last_seen,created_at,updated_at,deleted
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'scan_agent',now(),now(),now(),now(),0)
             ON CONFLICT (asset_ip,port,rule_id)
                 WHERE deleted=0 AND rule_id IS NOT NULL
             DO UPDATE SET severity=EXCLUDED.severity,description=EXCLUDED.description,
                 solution=COALESCE(scan_risks.solution,EXCLUDED.solution),
                 evidence=EXCLUDED.evidence,last_seen=now(),updated_at=now(),
                 status=CASE WHEN scan_risks.status='resolved' THEN EXCLUDED.status ELSE scan_risks.status END",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&risk.rule_id)
        .bind(target)
        .bind(risk.port as i32)
        .bind(&risk.severity)
        .bind(&risk.description)
        .bind(&risk.solution)
        .bind(&risk.initial_status)
        .bind(&risk.evidence)
        .execute(&mut **transaction)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    }
    Ok(())
}

async fn persist_agent_results(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    results: &[ScannerTargetResult],
    scanned_ports: &[u16],
) -> Result<(usize, usize), AppError> {
    let mut found_assets = 0_usize;
    let mut found_risks = 0_usize;
    let catalog = sqlx::query_as::<_, VulnerabilityCatalogRule>(
        "SELECT id,cve_id,product,match_type,exact_version,cpe_uri,
                version_start_including,version_start_excluding,
                version_end_including,version_end_excluding,
                severity,title,description,solution,reference_url,default_status
         FROM scan_vulnerability_rules WHERE enabled=true AND deleted=0",
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    for result in results {
        let mut ports = if result.ports.is_empty() {
            result
                .open_ports
                .iter()
                .copied()
                .map(|port| kairos_platform_scan_api::ScannerPortResult {
                    port,
                    service: port_to_service_name(port),
                    version: None,
                    banner: None,
                    fingerprint: None,
                })
                .collect::<Vec<_>>()
        } else {
            result.ports.clone()
        };
        ports.sort_by_key(|port| port.port);
        ports.dedup_by_key(|port| port.port);
        let mut detected_risks = detect_agent_risks(&ports);
        detected_risks.extend(detect_catalog_risks(&ports, &catalog));
        sync_agent_risks(transaction, &result.target, scanned_ports, &detected_risks).await?;
        found_risks += detected_risks.len();
        if ports.is_empty() {
            continue;
        }
        let asset_id = match sqlx::query_scalar::<_, i64>(
            "SELECT id FROM scan_assets WHERE ip=$1 AND deleted=0 ORDER BY id LIMIT 1",
        )
        .bind(&result.target)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        {
            Some(id) => id,
            None => sqlx::query_scalar::<_, i64>(
                "INSERT INTO scan_assets(name,ip,zone,ports,weight,labels,last_scanned)
                 VALUES($1,$1,'Internet','[]'::jsonb,50,'[]',now()) RETURNING id",
            )
            .bind(&result.target)
            .fetch_one(&mut **transaction)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?,
        };
        let port_info: Vec<PortInfo> = ports
            .iter()
            .map(|port| PortInfo {
                port: port.port,
                is_open: true,
                service: port
                    .service
                    .clone()
                    .or_else(|| port_to_service_name(port.port)),
                version: port.version.clone(),
                banner: port.banner.clone(),
                is_bound: false,
                system_name: None,
                middleware: None,
            })
            .collect();
        let ports_json = serde_json::to_value(&port_info)
            .map_err(|error| AppError::internal(error.to_string()))?;
        sqlx::query(
            "UPDATE scan_assets SET ports=$2,os=COALESCE($3,os),last_scanned=now(),updated_at=now()
             WHERE id=$1",
        )
        .bind(asset_id)
        .bind(ports_json)
        .bind(&result.os_hint)
        .execute(&mut **transaction)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
        sqlx::query(
            "UPDATE scan_port_details SET deleted=1,updated_at=now() WHERE ip=$1 AND deleted=0",
        )
        .bind(&result.target)
        .execute(&mut **transaction)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
        for port in ports {
            let port_number = port.port;
            let service = port.service.or_else(|| port_to_service_name(port_number));
            sqlx::query(
                "INSERT INTO scan_port_details(
                     id,asset_id,ip,port,protocol,status,service_name,service_version,
                     service_banner,fingerprint,created_at,updated_at,deleted
                 ) VALUES($1,$2,$3,$4,'TCP','open',$5,$6,$7,$8,now(),now(),0)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(asset_id.to_string())
            .bind(&result.target)
            .bind(port_number as i32)
            .bind(service)
            .bind(port.version)
            .bind(port.banner)
            .bind(port.fingerprint)
            .execute(&mut **transaction)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
        }
        found_assets += 1;
    }
    Ok((found_assets, found_risks))
}
async fn scanner_update(
    State(s): State<ScanState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateScannerRequest>,
) -> Result<Json<ApiResponse<ScannerConfig>>, AppError> {
    sqlx::query("UPDATE scan_scanners SET name=COALESCE($2,name),scanner_type=COALESCE($3,scanner_type),enabled=COALESCE($4,enabled),config=COALESCE($5,config),updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(&id).bind(&req.name).bind(&req.scanner_type).bind(req.enabled).bind(&req.config).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_scanners t WHERE id=$1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
async fn scanner_delete(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    sqlx::query("UPDATE scan_scanners SET deleted=1 WHERE id=$1")
        .bind(&id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new("deleted".into())))
}

// ── Zone CRUD ─────────────────────────────────────────────────────────────

async fn zone_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<ZoneConfig>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_cloud_zones t WHERE deleted=0 ORDER BY id",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let mut list: Vec<ZoneConfig> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    if list.is_empty() {
        list.push(ZoneConfig {
            id: "1".into(),
            name: "Intranet".into(),
            cidr: "192.168.0.0/16".into(),
            priority: 10,
            cloud_platform_id: None,
            cloud_platform_name: None,
            machine_room_id: None,
            machine_room_name: None,
        });
        list.push(ZoneConfig {
            id: "2".into(),
            name: "DMZ".into(),
            cidr: "10.0.0.0/8".into(),
            priority: 20,
            cloud_platform_id: None,
            cloud_platform_name: None,
            machine_room_id: None,
            machine_room_name: None,
        });
    }
    Ok(Json(ApiResponse::new(list)))
}
async fn zone_get(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ZoneConfig>>, AppError> {
    // For legacy IDs "1" and "2", return defaults
    match id.as_str() {
        "1" => Ok(Json(ApiResponse::new(ZoneConfig {
            id: "1".into(),
            name: "Intranet".into(),
            cidr: "192.168.0.0/16".into(),
            priority: 10,
            cloud_platform_id: None,
            cloud_platform_name: None,
            machine_room_id: None,
            machine_room_name: None,
        }))),
        "2" => Ok(Json(ApiResponse::new(ZoneConfig {
            id: "2".into(),
            name: "DMZ".into(),
            cidr: "10.0.0.0/8".into(),
            priority: 20,
            cloud_platform_id: None,
            cloud_platform_name: None,
            machine_room_id: None,
            machine_room_name: None,
        }))),
        _ => {
            let i: i32 = id
                .parse()
                .map_err(|_| AppError::bad_request("invalid id"))?;
            let v = sqlx::query_scalar::<_, Value>(
                "SELECT to_jsonb(t) FROM scan_cloud_zones t WHERE id=$1 AND deleted=0",
            )
            .bind(i)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .ok_or_else(|| AppError::not_found("not found"))?;
            Ok(Json(ApiResponse::new(
                serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
            )))
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateZoneRequest {
    name: String,
    cidr: String,
    priority: Option<i32>,
}

async fn zone_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateZoneRequest>,
) -> Result<Json<ApiResponse<ZoneConfig>>, AppError> {
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_cloud_zones(zone_name,zone_code,description) VALUES($1,$2,$3) RETURNING id").bind(&req.name).bind(&req.name).bind(&req.cidr).fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(ZoneConfig {
        id: id.to_string(),
        name: req.name,
        cidr: req.cidr,
        priority: req.priority.unwrap_or(50),
        cloud_platform_id: None,
        cloud_platform_name: None,
        machine_room_id: None,
        machine_room_name: None,
    })))
}
async fn zone_update(
    State(s): State<ScanState>,
    Path(id): Path<String>,
    Json(req): Json<CreateZoneRequest>,
) -> Result<Json<ApiResponse<ZoneConfig>>, AppError> {
    let i: i32 = id
        .parse()
        .map_err(|_| AppError::bad_request("invalid id"))?;
    sqlx::query("UPDATE scan_cloud_zones SET zone_name=COALESCE($2,zone_name),description=COALESCE($3,description) WHERE id=$1 AND deleted=0").bind(i).bind(&req.name).bind(&req.cidr).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(ZoneConfig {
        id,
        name: req.name,
        cidr: req.cidr,
        priority: req.priority.unwrap_or(50),
        cloud_platform_id: None,
        cloud_platform_name: None,
        machine_room_id: None,
        machine_room_name: None,
    })))
}
async fn zone_delete(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let i: i32 = id
        .parse()
        .map_err(|_| AppError::bad_request("invalid id"))?;
    sqlx::query("UPDATE scan_cloud_zones SET deleted=1 WHERE id=$1")
        .bind(i)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new("deleted".into())))
}

// ── Asset CRUD ────────────────────────────────────────────────────────────

async fn asset_list(
    State(s): State<ScanState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = asset_inventory::list(&s.pool).await?;
    // Apply keyword filter
    if let Some(keyword) = params.get("keyword").filter(|k| !k.is_empty()) {
        let kw = keyword.to_lowercase();
        let filtered: Vec<Value> = rows
            .into_iter()
            .filter(|v| {
                let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let ip = v.get("ip").and_then(|i| i.as_str()).unwrap_or("");
                name.to_lowercase().contains(&kw) || ip.contains(&kw)
            })
            .collect();
        Ok(Json(ApiResponse::new(filtered)))
    } else {
        Ok(Json(ApiResponse::new(rows)))
    }
}
async fn asset_get(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Asset>>, AppError> {
    let i: i32 = id
        .parse()
        .map_err(|_| AppError::bad_request("invalid id"))?;
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_assets t WHERE id=$1 AND deleted=0",
    )
    .bind(i)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}

#[derive(Debug, Deserialize)]
struct CreateAssetRequest {
    name: String,
    ip: String,
    zone: Option<String>,
    contact_person: Option<String>,
    contact_phone: Option<String>,
    owner: Option<String>,
    weight: Option<i32>,
    labels: Option<Vec<String>>,
    os: Option<String>,
    device_type: Option<String>,
}

async fn asset_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateAssetRequest>,
) -> Result<Json<ApiResponse<Asset>>, AppError> {
    let labels = req
        .labels
        .as_ref()
        .map(|l| serde_json::to_string(l).unwrap_or_default())
        .unwrap_or_default();
    let zone = req.zone.as_deref().unwrap_or("Intranet");
    let weight = req.weight.unwrap_or(50);
    let id: i32 = sqlx::query_scalar("INSERT INTO scan_assets(name,ip,zone,weight,labels,os,device_type,owner,contact_person,contact_phone) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id")
        .bind(&req.name).bind(&req.ip).bind(zone).bind(weight).bind(&labels).bind(&req.os).bind(&req.device_type).bind(&req.owner).bind(&req.contact_person).bind(&req.contact_phone)
        .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_assets t WHERE id=$1")
        .bind(id)
        .fetch_one(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
async fn asset_update(
    State(s): State<ScanState>,
    Path(id): Path<String>,
    Json(req): Json<CreateAssetRequest>,
) -> Result<Json<ApiResponse<Asset>>, AppError> {
    let i: i32 = id
        .parse()
        .map_err(|_| AppError::bad_request("invalid id"))?;
    let labels = req
        .labels
        .as_ref()
        .map(|l| serde_json::to_string(l).unwrap_or_default());
    sqlx::query("UPDATE scan_assets SET name=COALESCE($2,name),ip=COALESCE($3,ip),zone=COALESCE($4,zone),weight=COALESCE($5,weight),labels=COALESCE($6,labels),os=COALESCE($7,os),device_type=COALESCE($8,device_type),owner=COALESCE($9,owner),contact_person=COALESCE($10,contact_person),contact_phone=COALESCE($11,contact_phone),updated_at=now(),last_scanned=now() WHERE id=$1 AND deleted=0")
        .bind(i).bind(&req.name).bind(&req.ip).bind(&req.zone).bind(req.weight).bind(&labels).bind(&req.os).bind(&req.device_type).bind(&req.owner).bind(&req.contact_person).bind(&req.contact_phone)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    asset_get(State(s), Path(id)).await
}
async fn asset_delete(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let i: i32 = id
        .parse()
        .map_err(|_| AppError::bad_request("invalid id"))?;
    sqlx::query("UPDATE scan_assets SET deleted=1 WHERE id=$1")
        .bind(i)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new("deleted".into())))
}

// ── Task CRUD ─────────────────────────────────────────────────────────────

async fn task_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<ScanTask>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_tasks t WHERE deleted=0 ORDER BY created_at DESC",
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
async fn task_get(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ScanTask>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_tasks t WHERE id=$1 AND deleted=0",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
async fn task_create(
    State(s): State<ScanState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<ApiResponse<ScanTask>>, AppError> {
    task::validate_task_target(&req.target).map_err(|e| AppError::internal(e))?;
    let scanner_enabled: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM scan_scanners WHERE id=$1 AND enabled=true AND agent_status='online' AND last_heartbeat>now()-interval '90 seconds' AND deleted=0)",
    )
    .bind(&req.scanner_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if !scanner_enabled {
        return Err(AppError::bad_request("请选择一个已启用的扫描节点"));
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO scan_tasks(id,scanner_id,name,target,port_policy,service_detection,domain_brute,os_detection,site_identify) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&id).bind(&req.scanner_id).bind(&req.name).bind(&req.target).bind(&req.port_policy).bind(req.service_detection).bind(req.domain_brute).bind(req.os_detection).bind(req.site_identify)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(ScanTask {
        id,
        scanner_id: Some(req.scanner_id),
        name: req.name,
        target: req.target,
        status: kairos_platform_scan_api::TaskStatus::Pending,
        start_time: None,
        end_time: None,
        found_assets: 0,
        found_risks: 0,
        port_policy: req.port_policy,
        domain_brute: req.domain_brute,
        service_detection: req.service_detection,
        os_detection: req.os_detection,
        site_identify: req.site_identify,
    })))
}
async fn task_delete(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    sqlx::query("UPDATE scan_tasks SET deleted=1 WHERE id=$1")
        .bind(&id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new("deleted".into())))
}

// ── Scan execution ────────────────────────────────────────────────────────

async fn scan_execute(
    Json(_req): Json<ScanRequest>,
) -> Result<Json<ApiResponse<QuickScanResult>>, AppError> {
    Err(AppError::bad_request(
        "Gateway 不执行扫描；请创建绑定扫描节点的任务，由 Scan Agent 执行",
    ))
}

// ── Full task execution workflow ──────────────────────────────────────────

async fn task_execute(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let queued = sqlx::query_scalar::<_, String>(
        "UPDATE scan_tasks t SET status='pending', start_time=NULL, end_time=NULL,
             found_assets=0, found_risks=0, claim_token=NULL, lease_expires_at=NULL,
             error_message=NULL
         WHERE t.id=$1 AND t.deleted=0 AND t.status IN ('pending','failed','completed')
           AND EXISTS(
               SELECT 1 FROM scan_scanners s
               WHERE s.id=t.scanner_id AND s.enabled=true AND s.deleted=0
                 AND s.agent_status='online' AND s.last_heartbeat>now()-interval '90 seconds'
           )
         RETURNING t.id",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::bad_request("任务不存在、正在执行或扫描节点不在线"))?;
    Ok(Json(ApiResponse::new(json!({
        "task_id": queued,
        "status": "pending"
    }))))
}

async fn scan_results_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<QuickScanResult>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'ip', ip, 'is_alive', true,
             'open_ports', jsonb_agg(jsonb_build_object(
                 'port',port,'is_open',true,'service',service_name,'version',service_version,
                 'banner',service_banner,'is_bound',is_bound,'system_name',system_name,'middleware',middleware
             ) ORDER BY port),
             'fingerprint', max(fingerprint->>'http_title'), 'scanned_at', max(created_at)
         )
         FROM scan_port_details WHERE deleted=0
         GROUP BY ip ORDER BY max(created_at) DESC LIMIT 100",
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
async fn scan_result_get(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<QuickScanResult>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'ip', ip, 'is_alive', true,
             'open_ports', jsonb_agg(jsonb_build_object(
                 'port',port,'is_open',true,'service',service_name,'version',service_version,
                 'banner',service_banner,'is_bound',is_bound,'system_name',system_name,'middleware',middleware
             ) ORDER BY port),
             'fingerprint', max(fingerprint->>'http_title'), 'scanned_at', max(created_at)
         )
         FROM scan_port_details WHERE ip=$1 AND deleted=0 GROUP BY ip",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}

#[derive(Deserialize)]
struct CheckPortRequest {
    ip: String,
    port: u16,
}
async fn check_single_port(
    Json(_req): Json<CheckPortRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    Err(AppError::bad_request(
        "Gateway 不执行端口探测；请通过 Scan Agent 扫描任务执行",
    ))
}

fn port_to_service_name(port: u16) -> Option<String> {
    match port {
        21 => Some("FTP"),
        22 => Some("SSH"),
        23 => Some("Telnet"),
        25 => Some("SMTP"),
        53 => Some("DNS"),
        80 => Some("HTTP"),
        110 => Some("POP3"),
        143 => Some("IMAP"),
        443 => Some("HTTPS"),
        445 => Some("SMB"),
        993 => Some("IMAPS"),
        995 => Some("POP3S"),
        3306 => Some("MySQL"),
        3389 => Some("RDP"),
        5432 => Some("PostgreSQL"),
        6379 => Some("Redis"),
        8080 => Some("HTTP-Alt"),
        8443 => Some("HTTPS-Alt"),
        27017 => Some("MongoDB"),
        _ => None,
    }
    .map(String::from)
}

#[cfg(test)]
mod agent_protocol_tests {
    use super::{
        VulnerabilityCatalogRule, detect_agent_risks, detect_catalog_risks, scan_result_is_valid,
        target_is_allowed,
    };
    use kairos_platform_scan_api::{ScannerPortResult, ScannerTargetResult};

    #[test]
    fn accepts_only_exact_or_cidr_contained_agent_targets() {
        let declared = vec!["example.test".to_string(), "192.0.2.0/30".to_string()];
        assert!(target_is_allowed("example.test", &declared));
        assert!(target_is_allowed("192.0.2.1", &declared));
        assert!(!target_is_allowed("192.0.3.1", &declared));
        assert!(!target_is_allowed("other.test", &declared));
    }

    #[test]
    fn validates_bounded_detailed_scan_results() {
        let mut result = ScannerTargetResult {
            target: "192.0.2.1".to_string(),
            open_ports: vec![22],
            ports: vec![ScannerPortResult {
                port: 22,
                service: Some("SSH".to_string()),
                version: Some("OpenSSH_9.9".to_string()),
                banner: Some("SSH-2.0-OpenSSH_9.9".to_string()),
                fingerprint: None,
            }],
            os_hint: Some("Linux（服务证据推断）".to_string()),
            fingerprint: None,
        };
        assert!(scan_result_is_valid(&result));

        result.ports[0].port = 23;
        assert!(!scan_result_is_valid(&result));
        result.ports[0].port = 22;
        result.ports[0].banner = Some("x".repeat(2001));
        assert!(!scan_result_is_valid(&result));
    }

    #[test]
    fn derives_only_evidence_backed_scan_risks() {
        let ports = vec![
            ScannerPortResult {
                port: 6379,
                service: Some("Redis".to_string()),
                version: None,
                banner: Some("+PONG".to_string()),
                fingerprint: None,
            },
            ScannerPortResult {
                port: 8443,
                service: Some("HTTPS".to_string()),
                version: None,
                banner: Some("HTTP 200".to_string()),
                fingerprint: Some(serde_json::json!({
                    "tls": {"valid_now": false, "sha256": "abc"}
                })),
            },
            ScannerPortResult {
                port: 3306,
                service: Some("MySQL".to_string()),
                version: Some("8.4.0".to_string()),
                banner: Some("MySQL protocol 10 greeting".to_string()),
                fingerprint: None,
            },
        ];
        let risks = detect_agent_risks(&ports);
        assert_eq!(risks.len(), 2);
        assert_eq!(risks[0].rule_id, "redis-unauthenticated-ping");
        assert_eq!(risks[1].rule_id, "tls-certificate-time-invalid");
        assert!(risks.iter().all(|risk| risk.port != 3306));
    }

    #[test]
    fn matches_catalog_only_with_product_and_exact_version_evidence() {
        let rule = VulnerabilityCatalogRule {
            id: "cve-2011-2523-vsftpd-2.3.4".to_string(),
            cve_id: "CVE-2011-2523".to_string(),
            product: "vsftpd".to_string(),
            match_type: "exact".to_string(),
            exact_version: Some("2.3.4".to_string()),
            cpe_uri: Some("cpe:2.3:a:vsftpd:vsftpd:2.3.4:*:*:*:*:*:*:*".to_string()),
            version_start_including: None,
            version_start_excluding: None,
            version_end_including: None,
            version_end_excluding: None,
            severity: "Critical".to_string(),
            title: "vsftpd 2.3.4 可疑后门版本".to_string(),
            description: "需要核验发行包来源".to_string(),
            solution: "从可信来源替换".to_string(),
            reference_url: "https://nvd.nist.gov/vuln/detail/CVE-2011-2523".to_string(),
            default_status: "pending_review".to_string(),
        };
        let matched = ScannerPortResult {
            port: 21,
            service: Some("FTP".to_string()),
            version: Some("vsFTPd 2.3.4".to_string()),
            banner: Some("220 (vsFTPd 2.3.4)".to_string()),
            fingerprint: None,
        };
        let risks = detect_catalog_risks(&[matched], std::slice::from_ref(&rule));
        assert_eq!(risks.len(), 1);
        assert_eq!(risks[0].initial_status, "pending_review");
        assert_eq!(risks[0].evidence["confidence"], "version_only");

        let newer = ScannerPortResult {
            port: 21,
            service: Some("FTP".to_string()),
            version: Some("vsFTPd 3.0.5".to_string()),
            banner: None,
            fingerprint: None,
        };
        let unrelated = ScannerPortResult {
            port: 3306,
            service: Some("MySQL".to_string()),
            version: Some("2.3.4".to_string()),
            banner: None,
            fingerprint: None,
        };
        assert!(detect_catalog_risks(&[newer], std::slice::from_ref(&rule)).is_empty());
        assert!(detect_catalog_risks(&[unrelated], &[rule]).is_empty());
    }

    #[test]
    fn compares_versions_and_honors_cpe_ranges() {
        let mut rule = VulnerabilityCatalogRule {
            id: "range".into(),
            cve_id: "CVE-TEST".into(),
            product: "ignored-alias".into(),
            match_type: "cpe".into(),
            exact_version: None,
            cpe_uri: Some("cpe:2.3:a:apache:http_server:*:*:*:*:*:*:*:*".into()),
            version_start_including: Some("2.4.49".into()),
            version_start_excluding: None,
            version_end_including: None,
            version_end_excluding: Some("2.4.51".into()),
            severity: "High".into(),
            title: "range".into(),
            description: "range".into(),
            solution: "upgrade".into(),
            reference_url: "https://example.test".into(),
            default_status: "pending_review".into(),
        };
        assert!(super::catalog_rule_matches(
            &rule,
            "apache http_server",
            "2.4.49"
        ));
        assert!(super::catalog_rule_matches(
            &rule,
            "apache http_server",
            "2.4.50"
        ));
        assert!(!super::catalog_rule_matches(
            &rule,
            "apache http_server",
            "2.4.51"
        ));
        assert!(!super::catalog_rule_matches(&rule, "nginx", "2.4.50"));
        rule.version_end_excluding = Some("2.10.0".into());
        assert!(super::catalog_rule_matches(
            &rule,
            "apache http_server",
            "2.9.9"
        ));
    }
}
