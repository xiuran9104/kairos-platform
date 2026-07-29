//! Scan server — ported from rustset business modules.
//!
//! Provides asset scanning, network zone management, port scanning,
//! scanner configuration, and scan task management.

mod asset;
mod business_application;
mod business_resource;
mod cloud_adapter;
mod cloud_apsara;
mod cloud_hcs;
mod cloud_openstack;
mod cloud_platform;
mod cloud_provider_config;
mod cloud_service_asset;
mod cloud_tstack;
mod dashboard;
mod machine_room;
mod port_detail;
mod resource_ticket;
mod risk;
mod scanner;
mod security_product;
mod service_provider;
mod task;
mod zone;

use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::{
    Asset, BusinessResource, CloudPlatform, CloudProviderConfig, CloudSecurityGroup, CloudZone,
    CreateScannerRequest, CreateTaskRequest, MachineRoom, NetworkZone, PortDetail, PortInfo,
    QuickScanResult, ResourceTicket, Risk, ScanCapability, ScanRequest, ScanTask, ScannerConfig,
    SecurityProduct, ServiceProvider, UpdateScannerRequest, ZoneConfig,
};
use serde::Deserialize;
use serde_json::{Value, json};
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
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO scan_scanners(id,name,scanner_type,enabled,config,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(&id).bind(&req.name).bind(&req.scanner_type).bind(req.enabled.unwrap_or(true)).bind(&req.config).bind(&now).bind(&now)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(ScannerConfig {
        id,
        name: req.name,
        scanner_type: req.scanner_type,
        enabled: req.enabled.unwrap_or(true),
        config: req.config,
        created_at: Some(now.clone()),
        updated_at: Some(now),
    })))
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
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_assets t WHERE deleted=0 ORDER BY id DESC",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
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
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO scan_tasks(id,name,target,port_policy,service_detection,domain_brute,os_detection,site_identify) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(&id).bind(&req.name).bind(&req.target).bind(&req.port_policy).bind(req.service_detection).bind(req.domain_brute).bind(req.os_detection).bind(req.site_identify)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(ScanTask {
        id,
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
    State(s): State<ScanState>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<ApiResponse<QuickScanResult>>, AppError> {
    let ports = if req.ports.is_empty() {
        vec![22, 80, 443, 445, 3306, 3389, 5432, 6379, 8080, 8443, 27017]
    } else {
        req.ports
    };
    let result = scanner::tcp_scan(&req.target_ip, &ports).await;
    let open_ports: Vec<PortInfo> = result
        .into_iter()
        .map(|p| PortInfo {
            port: p,
            is_open: true,
            service: port_to_service_name(p),
            version: None,
            banner: None,
            is_bound: false,
            system_name: None,
            middleware: None,
        })
        .collect();
    let r = QuickScanResult {
        ip: req.target_ip.clone(),
        is_alive: !open_ports.is_empty(),
        open_ports,
        fingerprint: None,
        scanned_at: chrono::Utc::now(),
    };
    Ok(Json(ApiResponse::new(r)))
}

// ── Full task execution workflow ──────────────────────────────────────────

async fn task_execute(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // Load task
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM scan_tasks t WHERE id=$1 AND deleted=0",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("task not found"))?;
    let mut task: ScanTask =
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?;

    // Validate
    task::validate_task_target(&task.target).map_err(|e| AppError::internal(e))?;

    // Start
    task::start_task(&mut task);
    sqlx::query("UPDATE scan_tasks SET status='running',start_time=now() WHERE id=$1")
        .bind(&task.id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    // Pick port list
    let ports: Vec<u16> = match task.port_policy.as_str() {
        "TOP100" => scanner::common_ports::TOP100.to_vec(),
        "TOP1000" => scanner::common_ports::TOP1000.to_vec(),
        _ => vec![22, 80, 443, 445, 3306, 3389, 5432, 6379, 8080, 8443, 27017],
    };

    // Load zone configs
    let zone_rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('id',id,'name',name,'cidr',cidr,'priority',priority) FROM scan_cloud_zones WHERE deleted=0",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let zones: Vec<ZoneConfig> = zone_rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    // Execute scan for each target IP
    let targets = task::parse_targets(&task.target);
    let mut total_assets = 0usize;
    let mut total_risks = 0usize;

    for target_ip in &targets {
        let open_ports = scanner::tcp_scan(target_ip, &ports).await;
        if open_ports.is_empty() {
            continue;
        }

        // Determine zone
        let zone = zone::determine_zone(target_ip, &zones);

        // Look up or create asset
        let existing = sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_build_object('id',id,'name',name,'ip',ip,'zone',zone,'ports',ports,'weight',weight,'labels',labels,'os',os,'device_type',device_type,'owner',owner) FROM scan_assets WHERE ip=$1 AND deleted=0",
        )
        .bind(target_ip)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

        let zone_str = serde_json::to_string(&zone).unwrap_or_else(|_| "\"Internet\"".into());
        let mut asset = if let Some(av) = existing {
            serde_json::from_value::<Asset>(av).unwrap_or_else(|_| Asset {
                id: None,
                name: target_ip.clone(),
                ip: target_ip.clone(),
                zone: zone.clone(),
                ports: vec![],
                last_scanned: None,
                contact_person: None,
                contact_phone: None,
                owner: None,
                weight: 50,
                labels: vec![],
                os: None,
                device_type: None,
            })
        } else {
            sqlx::query("INSERT INTO scan_assets(name,ip,zone,ports,weight,labels) VALUES($1,$2,$3::jsonb,'[]'::jsonb,$4,'[]'::jsonb)")
                .bind(target_ip).bind(target_ip).bind(&zone_str)
                .bind(50i32)
                .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
            Asset {
                id: None,
                name: target_ip.clone(),
                ip: target_ip.clone(),
                zone,
                ports: vec![],
                last_scanned: None,
                contact_person: None,
                contact_phone: None,
                owner: None,
                weight: 50,
                labels: vec![],
                os: None,
                device_type: None,
            }
        };

        // Retrieve the newly inserted asset id for updates
        let asset_id: Option<i32> = if asset.id.is_none() {
            sqlx::query_scalar::<_, i32>(
                "SELECT id FROM scan_assets WHERE ip=$1 AND deleted=0 ORDER BY id DESC LIMIT 1",
            )
            .bind(&asset.ip)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
        } else {
            asset.id
        };

        // Map ports with service names
        let scan_ports: Vec<PortInfo> = open_ports
            .into_iter()
            .map(|p| PortInfo {
                port: p,
                is_open: true,
                service: port_to_service_name(p),
                version: None,
                banner: None,
                is_bound: false,
                system_name: None,
                middleware: None,
            })
            .collect();

        // Merge into asset
        asset::merge_scan_ports(&mut asset, scan_ports);
        let risk_score = asset::asset_risk_score(&asset);
        total_assets += 1;

        // Persist updated ports
        if let Some(aid) = asset_id {
            let ports_json = serde_json::to_value(&asset.ports)
                .map_err(|e| AppError::internal(e.to_string()))?;
            sqlx::query("UPDATE scan_assets SET ports=$2,last_scanned=now() WHERE id=$1")
                .bind(aid)
                .bind(&ports_json)
                .execute(&s.pool)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }

        // Record open ports as scan results
        for port in &asset.ports {
            if port.is_open {
                let detail_id = Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO scan_port_details(id,ip,port,service,version,banner,is_open,risk_score,scanned_at) VALUES($1,$2,$3,$4,$5,$6,true,$7,now())")
                    .bind(&detail_id).bind(&asset.ip).bind(port.port as i32)
                    .bind(&port.service).bind(&port.version).bind(&port.banner)
                    .bind(risk_score as i32)
                    .execute(&s.pool).await
                    .map_err(|e| AppError::internal(e.to_string()))?;
                total_risks += 1;
            }
        }
    }

    // Complete
    task::complete_task(&mut task, total_assets, total_risks);
    sqlx::query(
        "UPDATE scan_tasks SET status='completed',end_time=now(),found_assets=$2,found_risks=$3 WHERE id=$1",
    )
    .bind(&task.id).bind(total_assets as i32).bind(total_risks as i32)
    .execute(&s.pool).await
    .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(ApiResponse::new(json!({
        "task_id": task.id,
        "status": "completed",
        "assets_found": total_assets,
        "risks_found": total_risks,
    }))))
}

async fn scan_results_list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<QuickScanResult>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_port_details t WHERE deleted=0 ORDER BY created_at DESC LIMIT 100").fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
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
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM scan_port_details t WHERE ip=$1 AND deleted=0 ORDER BY created_at DESC LIMIT 1").bind(&id).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.ok_or_else(|| AppError::not_found("not found"))?;
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
    Json(req): Json<CheckPortRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let is_open = scanner::check_port(&req.ip, req.port).await;
    Ok(Json(ApiResponse::new(json!({
        "ip": req.ip,
        "port": req.port,
        "is_open": is_open,
        "service": port_to_service_name(req.port),
    }))))
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
