//! Scan API types — ported from rustset shared types.
//!
//! Network zones, scanner configs, asset records, port info, scan tasks and results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Network zone ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkZone {
    Internet,
    #[serde(rename = "DMZ")]
    Dmz,
    Intranet,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneConfig {
    pub id: String,
    pub name: String,
    pub cidr: String,
    pub priority: i32,
    pub cloud_platform_id: Option<i32>,
    pub cloud_platform_name: Option<String>,
    pub machine_room_id: Option<i32>,
    pub machine_room_name: Option<String>,
}

// ── Port info ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortInfo {
    pub port: u16,
    pub is_open: bool,
    pub service: Option<String>,
    pub version: Option<String>,
    pub banner: Option<String>,
    pub is_bound: bool,
    pub system_name: Option<String>,
    pub middleware: Option<String>,
}

// ── Asset ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: Option<i32>,
    pub name: String,
    pub ip: String,
    pub zone: NetworkZone,
    pub ports: Vec<PortInfo>,
    pub last_scanned: Option<DateTime<Utc>>,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub owner: Option<String>,
    pub weight: i32,
    pub labels: Vec<String>,
    pub os: Option<String>,
    pub device_type: Option<String>,
}

// ── Scan task ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanTask {
    pub id: String,
    pub scanner_id: Option<String>,
    pub name: String,
    pub target: String,
    pub status: TaskStatus,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub found_assets: usize,
    pub found_risks: usize,
    pub port_policy: String,
    pub domain_brute: bool,
    pub service_detection: bool,
    pub os_detection: bool,
    pub site_identify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub scanner_id: String,
    pub name: String,
    pub target: String,
    pub port_policy: String,
    pub domain_brute: bool,
    pub service_detection: bool,
    pub os_detection: bool,
    pub site_identify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    pub target_ip: String,
    pub ports: Vec<u16>,
}

// ── Scanner config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub id: String,
    pub name: String,
    pub scanner_type: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub agent_status: Option<String>,
    #[serde(default)]
    pub agent_version: Option<String>,
    #[serde(default)]
    pub agent_hostname: Option<String>,
    #[serde(default)]
    pub agent_ip: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub registered_at: Option<String>,
    #[serde(default)]
    pub last_heartbeat: Option<String>,
    #[serde(default)]
    pub registration_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScannerRequest {
    pub name: String,
    pub scanner_type: String,
    pub enabled: Option<bool>,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScannerRequest {
    pub name: Option<String>,
    pub scanner_type: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterScannerAgentRequest {
    pub token: String,
    pub agent_version: String,
    pub hostname: String,
    pub ip_address: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerHeartbeatRequest {
    pub node_id: String,
    pub token: String,
}

// ── Risk ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskStatus {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "verified")]
    Verified,
    #[serde(rename = "resolved")]
    Resolved,
    #[serde(rename = "ignored")]
    Ignored,
    #[serde(rename = "false_positive")]
    FalsePositive,
    #[serde(rename = "pending_review")]
    PendingReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Risk {
    pub id: String,
    pub asset_ip: String,
    pub port: u16,
    pub severity: String,
    pub description: String,
    pub solution: Option<String>,
    pub status: RiskStatus,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub assigned_to: Option<String>,
}

// ── Quick scan result ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickScanResult {
    pub ip: String,
    pub is_alive: bool,
    pub open_ports: Vec<PortInfo>,
    pub fingerprint: Option<String>,
    pub scanned_at: DateTime<Utc>,
}

// ── Service fingerprint ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceFingerprint {
    pub service: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub extra_info: Option<String>,
    pub cpe: Option<String>,
    pub os_match: Option<String>,
    pub confidence: i32,
}

// ── Port detail ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDetail {
    pub id: String,
    pub asset_id: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    pub status: String,
    pub service_name: Option<String>,
    pub service_version: Option<String>,
    pub service_banner: Option<String>,
    pub system_name: Option<String>,
    pub system_type: Option<String>,
    pub middleware: Option<String>,
    pub framework: Option<String>,
    pub language: Option<String>,
    pub environment: Option<String>,
    pub department: Option<String>,
    pub owner: Option<String>,
    pub owner_contact: Option<String>,
    pub vulnerability_level: Option<String>,
    pub has_vulnerability: bool,
    pub vulnerability_count: i32,
    pub fingerprint: Option<ServiceFingerprint>,
    pub is_bound: bool,
    pub notes: Option<String>,
    pub last_scanned: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ── Advanced scan ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScanStrategy {
    #[serde(rename = "quick")]
    Quick,
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "custom")]
    Custom(Vec<u16>),
    #[serde(rename = "cloud")]
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScanEngine {
    #[serde(rename = "basic_tcp")]
    BasicTcp,
    #[serde(rename = "rustscan")]
    RustScan,
    #[serde(rename = "nmap")]
    Nmap,
    #[serde(rename = "hybrid")]
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedScanConfig {
    pub strategy: ScanStrategy,
    pub engine: ScanEngine,
    pub concurrency: u32,
    pub timeout_ms: u64,
    pub service_detection: bool,
    pub os_detection: bool,
    pub web_fingerprint: bool,
    pub vulnerability_scan: bool,
    pub cloud_tag_sync: bool,
    pub rate_limit: Option<u32>,
}

impl Default for AdvancedScanConfig {
    fn default() -> Self {
        Self {
            strategy: ScanStrategy::Standard,
            engine: ScanEngine::Hybrid,
            concurrency: 1000,
            timeout_ms: 5000,
            service_detection: true,
            os_detection: false,
            web_fingerprint: true,
            vulnerability_scan: false,
            cloud_tag_sync: true,
            rate_limit: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedScanTask {
    pub id: String,
    pub name: String,
    pub targets: Vec<String>,
    pub config: AdvancedScanConfig,
    pub status: TaskStatus,
    pub progress: f32,
    pub current_target: Option<String>,
    pub scanned_count: u32,
    pub total_count: u32,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub results: Vec<QuickScanResult>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAdvancedScanRequest {
    pub name: String,
    pub targets: Vec<String>,
    pub strategy: ScanStrategy,
    pub engine: ScanEngine,
    pub concurrency: Option<u32>,
    pub timeout_ms: Option<u64>,
    pub service_detection: Option<bool>,
    pub os_detection: Option<bool>,
    pub web_fingerprint: Option<bool>,
    pub cloud_tag_sync: Option<bool>,
}

// ── Scan capability ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ScanCapability {
    pub module: &'static str,
    pub capabilities: Vec<&'static str>,
}

impl Default for ScanCapability {
    fn default() -> Self {
        Self {
            module: "scan",
            capabilities: vec![
                "scanner",
                "asset",
                "task",
                "risk",
                "zone",
                "machine-room",
                "service-provider",
                "security-product",
                "cloud-platform",
                "resource-ticket",
                "dashboard",
            ],
        }
    }
}

// ── Machine room ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineRoom {
    pub id: i32,
    pub room_name: String,
    pub room_code: String,
    pub facility_type: String,
    pub address: String,
    pub platform_id: i32,
    pub room_type: String,
    pub contact_person: String,
    pub contact_phone: String,
    pub floor: Option<String>,
    pub cabinet_count: Option<i32>,
    pub area_size: Option<String>,
    pub remarks: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMachineRoomRequest {
    pub room_name: String,
    pub room_code: String,
    pub facility_type: String,
    pub address: String,
    pub platform_id: i32,
    pub room_type: String,
    pub contact_person: String,
    pub contact_phone: String,
    pub floor: Option<String>,
    pub cabinet_count: Option<i32>,
    pub area_size: Option<String>,
    pub remarks: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMachineRoomRequest {
    pub room_name: Option<String>,
    pub room_code: Option<String>,
    pub facility_type: Option<String>,
    pub address: Option<String>,
    pub platform_id: Option<i32>,
    pub room_type: Option<String>,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub floor: Option<String>,
    pub cabinet_count: Option<i32>,
    pub area_size: Option<String>,
    pub remarks: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CabinetRow {
    pub id: i32,
    pub machine_room_id: i32,
    pub row_name: String,
    pub row_code: String,
    pub sort: i32,
    pub status: String,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cabinet {
    pub id: i32,
    pub row_id: i32,
    pub cabinet_name: String,
    pub cabinet_code: String,
    pub total_u: i32,
    pub power_capacity_kw: Option<f64>,
    pub status: String,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCabinetRowRequest {
    pub machine_room_id: i32,
    pub row_name: String,
    pub row_code: String,
    pub sort: Option<i32>,
    pub status: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCabinetRequest {
    pub row_id: i32,
    pub cabinet_name: String,
    pub cabinet_code: String,
    pub total_u: i32,
    pub power_capacity_kw: Option<f64>,
    pub status: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureMachineRoomCabinetsRequest {
    pub machine_room_id: i32,
    pub row_count: i32,
    pub cabinets_per_row: i32,
    pub cabinet_total_u: i32,
}

// ── Service provider ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceProvider {
    pub id: i32,
    pub provider_name: String,
    pub provider_code: String,
    pub short_name: String,
    pub logo_url: Option<String>,
    pub contact_person: String,
    pub contact_phone: String,
    pub contact_email: String,
    pub headquarters: String,
    pub service_area: String,
    pub business_license: String,
    pub remarks: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateServiceProviderRequest {
    pub provider_name: String,
    pub provider_code: String,
    pub short_name: String,
    pub logo_url: Option<String>,
    pub contact_person: String,
    pub contact_phone: String,
    pub contact_email: String,
    pub headquarters: String,
    pub service_area: String,
    pub business_license: String,
    pub remarks: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateServiceProviderRequest {
    pub provider_name: Option<String>,
    pub provider_code: Option<String>,
    pub short_name: Option<String>,
    pub logo_url: Option<String>,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub contact_email: Option<String>,
    pub headquarters: Option<String>,
    pub service_area: Option<String>,
    pub business_license: Option<String>,
    pub remarks: Option<String>,
    pub status: Option<String>,
}

// ── Security product ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfrastructureDevice {
    pub id: i32,
    pub device_type: String,
    pub name: String,
    pub category: String,
    pub vendor: String,
    pub model: String,
    pub serial_number: Option<String>,
    pub management_ip: Option<String>,
    pub business_ip: Option<String>,
    pub machine_room_id: Option<i32>,
    pub rack_position: Option<String>,
    pub status: String,
    pub specification: Option<String>,
    pub capacity: Option<String>,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub remarks: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInfrastructureDeviceRequest {
    pub device_type: String,
    pub name: String,
    pub category: String,
    pub vendor: String,
    pub model: String,
    pub serial_number: Option<String>,
    pub management_ip: Option<String>,
    pub business_ip: Option<String>,
    pub machine_room_id: Option<i32>,
    pub rack_position: Option<String>,
    pub status: Option<String>,
    pub specification: Option<String>,
    pub capacity: Option<String>,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityProduct {
    pub id: i32,
    pub name: String,
    pub category: String,
    pub vendor: String,
    pub model: String,
    pub version: String,
    pub serial_number: Option<String>,
    pub license_type: String,
    pub license_expiry: Option<String>,
    pub management_ip: Option<String>,
    pub deployment_mode: String,
    pub cloud_platform_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub provider_id: i32,
    pub status: String,
    pub features: Option<String>,
    pub throughput: Option<String>,
    pub contact_person: String,
    pub contact_phone: String,
    pub remarks: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSecurityProductRequest {
    pub name: String,
    pub category: String,
    pub vendor: String,
    pub model: String,
    pub version: String,
    pub serial_number: Option<String>,
    pub license_type: String,
    pub license_expiry: Option<String>,
    pub management_ip: Option<String>,
    pub deployment_mode: String,
    pub cloud_platform_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub provider_id: Option<i32>,
    pub status: Option<String>,
    pub features: Option<Vec<String>>,
    pub throughput: Option<String>,
    pub contact_person: String,
    pub contact_phone: String,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSecurityProductRequest {
    pub name: Option<String>,
    pub category: Option<String>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub version: Option<String>,
    pub serial_number: Option<String>,
    pub license_type: Option<String>,
    pub license_expiry: Option<String>,
    pub management_ip: Option<String>,
    pub deployment_mode: Option<String>,
    pub cloud_platform_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub provider_id: Option<i32>,
    pub status: Option<String>,
    pub features: Option<Vec<String>>,
    pub throughput: Option<String>,
    pub contact_person: Option<String>,
    pub contact_phone: Option<String>,
    pub remarks: Option<String>,
}

// ── Cloud zone & platform ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudZone {
    pub id: Option<i32>,
    pub zone_name: String,
    pub zone_code: String,
    pub zone_type: String,
    pub provider_id: i32,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCloudZoneRequest {
    pub zone_name: String,
    pub zone_code: String,
    pub zone_type: String,
    pub provider_id: i32,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCloudZoneRequest {
    pub zone_name: Option<String>,
    pub zone_code: Option<String>,
    pub zone_type: Option<String>,
    pub provider_id: Option<i32>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPlatform {
    pub id: Option<i32>,
    pub zone_id: i32,
    pub platform_name: String,
    pub platform_code: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCloudPlatformRequest {
    pub zone_id: i32,
    pub platform_name: String,
    pub platform_code: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCloudPlatformRequest {
    pub zone_id: Option<i32>,
    pub platform_name: Option<String>,
    pub platform_code: Option<String>,
    pub description: Option<String>,
}

// ── Cloud provider config ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudProviderVendor {
    #[serde(rename = "aliyun")]
    Aliyun,
    #[serde(rename = "tencent")]
    Tencent,
    #[serde(rename = "huawei")]
    Huawei,
    #[serde(rename = "hcs")]
    Hcs,
    #[serde(rename = "apsara")]
    Apsara,
    #[serde(rename = "tstack")]
    TStack,
    #[serde(rename = "openstack")]
    OpenStack,
    #[serde(rename = "aws")]
    Aws,
    #[serde(rename = "azure")]
    Azure,
    #[serde(rename = "gcp")]
    Gcp,
    #[serde(rename = "custom")]
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudProviderConfigStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "testing")]
    Testing,
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudProviderConfig {
    pub id: Option<i32>,
    pub platform_id: i32,
    pub provider: CloudProviderVendor,
    pub region_id: String,
    pub region_name: String,
    pub available_zones: Vec<String>,
    pub account_name: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    pub auth_type: String,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub auth_domain: Option<String>,
    pub project_id: Option<String>,
    pub status: CloudProviderConfigStatus,
    pub remarks: Option<String>,
    pub last_test_time: Option<String>,
    pub last_test_result: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCloudProviderConfigRequest {
    pub platform_id: i32,
    pub provider: CloudProviderVendor,
    pub region_id: String,
    pub region_name: String,
    pub available_zones: Option<Vec<String>>,
    pub account_name: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    pub auth_type: Option<String>,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub auth_domain: Option<String>,
    pub project_id: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCloudProviderConfigRequest {
    pub platform_id: Option<i32>,
    pub provider: Option<CloudProviderVendor>,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub available_zones: Option<Vec<String>>,
    pub account_name: Option<String>,
    pub access_key_id: Option<String>,
    pub access_key_secret: Option<String>,
    pub auth_type: Option<String>,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub auth_domain: Option<String>,
    pub project_id: Option<String>,
    pub status: Option<CloudProviderConfigStatus>,
    pub remarks: Option<String>,
}

// ── Resource ticket ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    PendingApproval,
    Approved,
    Rejected,
    PendingProvision,
    Provisioning,
    PendingDelivery,
    Delivered,
    Archived,
}

impl TicketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TicketStatus::PendingApproval => "pending_approval",
            TicketStatus::Approved => "approved",
            TicketStatus::Rejected => "rejected",
            TicketStatus::PendingProvision => "pending_provision",
            TicketStatus::Provisioning => "provisioning",
            TicketStatus::PendingDelivery => "pending_delivery",
            TicketStatus::Delivered => "delivered",
            TicketStatus::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTicket {
    pub id: Option<i32>,
    pub ticket_type: String,
    pub risk_level: String,
    pub target_resource_id: Option<i32>,
    pub target_config: Option<String>,
    pub maintenance_window: Option<String>,
    pub allow_interruption: bool,
    pub backup_confirmed: bool,
    pub rollback_plan: Option<String>,
    pub retention_until: Option<String>,
    pub approval_stage: i32,
    pub approval_total: i32,
    pub current_approval_role: Option<String>,
    pub resource_type: String,
    pub ecs_name: String,
    pub ticket_status: TicketStatus,
    pub provider_id: Option<i32>,
    pub provider_name: Option<String>,
    pub cloud_platform_id: Option<i32>,
    pub cloud_platform_name: Option<String>,
    pub machine_room_id: Option<i32>,
    pub machine_room_name: Option<String>,
    pub cloud_region: Option<String>,
    pub cloud_category: Option<String>,
    pub zone_name: Option<String>,
    pub zone_cabinet: Option<String>,
    pub rack_units: i32,
    pub customer_name: Option<String>,
    pub application_name: Option<String>,
    pub contract_name: Option<String>,
    pub ecs_type: Option<String>,
    pub ecs_os: Option<String>,
    pub resource_count: i32,
    pub cpu_cores: i32,
    pub memory_gb: i32,
    pub system_disk: Option<String>,
    pub system_disk_size_gb: i32,
    pub data_disk: Option<String>,
    pub expire_at: Option<String>,
    pub has_security_product: bool,
    pub security_products: Option<String>,
    pub ip_address: Option<String>,
    pub delivery_status: Option<String>,
    pub remarks: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub created_by: String,
    pub applicant_name: Option<String>,
    pub organization_id: Option<i32>,
    pub organization_name: Option<String>,
    pub department_id: Option<i32>,
    pub department_name: Option<String>,
    pub approver: Option<String>,
    pub approve_time: Option<String>,
    pub approve_comment: Option<String>,
    pub provisioner: Option<String>,
    pub provision_time: Option<String>,
    pub provision_details: Option<String>,
    pub deliverer: Option<String>,
    pub deliver_time: Option<String>,
    pub deliver_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResourceTicketRequest {
    pub ticket_type: Option<String>,
    pub target_resource_id: Option<i32>,
    pub target_config: Option<String>,
    pub maintenance_window: Option<String>,
    pub allow_interruption: Option<bool>,
    pub backup_confirmed: Option<bool>,
    pub rollback_plan: Option<String>,
    pub retention_until: Option<String>,
    pub resource_type: String,
    pub ecs_name: String,
    pub provider_id: Option<i32>,
    pub cloud_platform_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub cloud_region: Option<String>,
    pub cloud_category: Option<String>,
    pub zone_name: Option<String>,
    pub zone_cabinet: Option<String>,
    pub rack_units: Option<i32>,
    pub customer_name: Option<String>,
    pub application_name: Option<String>,
    pub contract_name: Option<String>,
    pub ecs_type: Option<String>,
    pub ecs_os: Option<String>,
    pub resource_count: Option<i32>,
    pub cpu_cores: Option<i32>,
    pub memory_gb: Option<i32>,
    pub system_disk: Option<String>,
    pub system_disk_size_gb: Option<i32>,
    pub data_disk: Option<String>,
    pub expire_at: Option<String>,
    pub has_security_product: Option<bool>,
    pub security_products: Option<String>,
    pub ip_address: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResourceTicketRequest {
    pub ecs_name: Option<String>,
    pub provider_id: Option<i32>,
    pub cloud_platform_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub cloud_region: Option<String>,
    pub cloud_category: Option<String>,
    pub zone_name: Option<String>,
    pub zone_cabinet: Option<String>,
    pub rack_units: Option<i32>,
    pub customer_name: Option<String>,
    pub application_name: Option<String>,
    pub contract_name: Option<String>,
    pub ecs_type: Option<String>,
    pub ecs_os: Option<String>,
    pub resource_count: Option<i32>,
    pub cpu_cores: Option<i32>,
    pub memory_gb: Option<i32>,
    pub system_disk: Option<String>,
    pub system_disk_size_gb: Option<i32>,
    pub data_disk: Option<String>,
    pub expire_at: Option<String>,
    pub has_security_product: Option<bool>,
    pub security_products: Option<String>,
    pub ip_address: Option<String>,
    pub delivery_status: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveTicketRequest {
    pub approved: bool,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionTicketRequest {
    pub details: Option<String>,
    pub config_id: Option<i32>,
    pub image_id: Option<String>,
    pub flavor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverTicketRequest {
    pub comment: Option<String>,
}

// ── Dashboard ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub asset_count: u64,
    pub scanned_asset_count: u64,
    pub task_count: u64,
    pub running_task_count: u64,
    pub risk_count: u64,
    pub open_risk_count: u64,
    pub database_connected: bool,
    pub version: String,
}

// ── Business application ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessApplication {
    pub id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBusinessApplicationRequest {
    pub name: String,
    pub description: Option<String>,
}

// ── Cloud service asset ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudServiceAsset {
    pub id: String,
    pub provider_config_id: i32,
    pub provider: String,
    pub account_name: String,
    pub asset_type: String,
    pub source_type: String,
    pub name: String,
    pub instance_id: String,
    pub status: String,
    pub region: Option<String>,
    pub region_id: Option<String>,
    pub availability_zone: Option<String>,
    pub cloud_platform: String,
    pub cloud_zone: String,
    pub instance_type: String,
    pub cpu_cores: u32,
    pub memory_gb: u32,
    pub system_disk_type: String,
    pub system_disk_size_gb: u32,
    pub os_type: String,
    pub os_name: String,
    pub ip_address: String,
    pub public_ip: Option<String>,
    pub customer_name: String,
    pub department: Option<String>,
    pub project: Option<String>,
    pub application_name: Option<String>,
    pub owner_name: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

// ── Physical machine info ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhysicalMachineInfo {
    pub id: Option<i32>,
    pub business_resource_id: Option<i32>,
    pub serial_number: Option<String>,
    pub rack_location: Option<String>,
    pub hardware_model: Option<String>,
    pub warranty_expiry: Option<DateTime<Utc>>,
    pub agent_status: Option<String>,
    pub ipmi_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudVirtualMachineInfo {
    pub id: Option<i32>,
    pub business_resource_id: Option<i32>,
    pub billing_mode: Option<String>,
    pub expire_time: Option<DateTime<Utc>>,
    pub charge_type: Option<String>,
    pub instance_charge_type: Option<String>,
    pub internet_charge_type: Option<String>,
    pub internet_max_bandwidth_out: Option<i32>,
    pub image_id: Option<String>,
    pub v_switch_id: Option<String>,
    pub vpc_id: Option<String>,
    pub security_group_ids: Option<Vec<String>>,
}

// ── Business resource (云资源/物理机受理单) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessResource {
    pub id: Option<i32>,
    pub resource_type: String,
    pub ecs_name: String,
    pub ecs_status: String,
    pub resource_id: String,
    pub cloud_region: String,
    pub cloud_category: String,
    pub cloud_provider_config_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub cabinet_id: Option<i32>,
    pub cabinet_name: Option<String>,
    pub chassis_name: Option<String>,
    pub chassis_slot: Option<String>,
    pub rack_start_u: Option<i32>,
    pub rack_end_u: Option<i32>,
    pub deployment_type: String,
    pub management_ip: Option<String>,
    pub business_ip: Option<String>,
    pub network_cidr: Option<String>,
    pub gateway: Option<String>,
    pub vlan_id: Option<String>,
    pub dns_servers: Option<String>,
    pub mac_address: Option<String>,
    pub switch_name: Option<String>,
    pub switch_port: Option<String>,
    pub zone_name: Option<String>,
    pub platform_name: Option<String>,
    pub county_city: Option<String>,
    pub vdc_name: Option<String>,
    pub customer_name: String,
    pub application_name: Option<String>,
    pub contract_name: Option<String>,
    pub instance_id: String,
    pub ecs_type: String,
    pub ecs_os: String,
    pub cpu_cores: u32,
    pub memory_gb: u32,
    pub system_disk: String,
    pub system_disk_size_gb: u32,
    pub data_disk: Option<String>,
    pub completion_time: Option<DateTime<Utc>>,
    pub release_time: Option<DateTime<Utc>>,
    pub has_security_product: bool,
    pub ip_address: String,
    pub ecs_login_method: Option<String>,
    pub ecs_login_username: Option<String>,
    pub ecs_initial_password: Option<String>,
    pub bastion_address: Option<String>,
    pub bastion_admin_account: Option<String>,
    pub bastion_initial_password: Option<String>,
    pub physical_machine_info: Option<PhysicalMachineInfo>,
    pub cloud_vm_info: Option<CloudVirtualMachineInfo>,
    pub remarks: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub applicant: Option<String>,
    pub department: Option<String>,
    pub approver: Option<String>,
    pub approval_time: Option<DateTime<Utc>>,
    pub approval_remarks: Option<String>,
    pub rejection_reason: Option<String>,
    pub bandwidth_mbps: Option<u32>,
    pub bandwidth_type: Option<String>,
    pub public_ip_count: Option<u32>,
    pub network_type: Option<String>,
    pub project_name: Option<String>,
    pub project_code: Option<String>,
    pub business_owner: Option<String>,
    pub tech_owner: Option<String>,
    pub contact_phone: Option<String>,
    pub billing_method: Option<String>,
    pub purchase_duration: Option<u32>,
    pub cost_center: Option<String>,
    pub security_level: Option<String>,
    pub data_sensitivity: Option<String>,
    pub purpose: Option<String>,
    pub expected_delivery_time: Option<DateTime<Utc>>,
    pub application_status: Option<String>,
    pub delivery_status: Option<String>,
    pub delivery_confirmed_at: Option<DateTime<Utc>>,
    pub delivery_confirmed_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBusinessResourceRequest {
    pub resource_type: String,
    pub ecs_name: String,
    pub ecs_status: String,
    pub resource_id: Option<String>,
    pub cloud_region: String,
    pub cloud_category: String,
    pub cloud_provider_config_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub cabinet_id: Option<i32>,
    pub cabinet_name: Option<String>,
    pub chassis_name: Option<String>,
    pub chassis_slot: Option<String>,
    pub rack_start_u: Option<i32>,
    pub rack_end_u: Option<i32>,
    pub deployment_type: Option<String>,
    pub management_ip: Option<String>,
    pub business_ip: Option<String>,
    pub network_cidr: Option<String>,
    pub gateway: Option<String>,
    pub vlan_id: Option<String>,
    pub dns_servers: Option<String>,
    pub mac_address: Option<String>,
    pub switch_name: Option<String>,
    pub switch_port: Option<String>,
    pub zone_name: Option<String>,
    pub platform_name: Option<String>,
    pub county_city: Option<String>,
    pub vdc_name: Option<String>,
    pub customer_name: String,
    pub application_name: Option<String>,
    pub contract_name: Option<String>,
    pub instance_id: Option<String>,
    pub ecs_type: String,
    pub ecs_os: String,
    pub cpu_cores: u32,
    pub memory_gb: u32,
    pub system_disk: String,
    pub system_disk_size_gb: u32,
    pub data_disk: Option<String>,
    pub completion_time: Option<DateTime<Utc>>,
    pub release_time: Option<DateTime<Utc>>,
    pub has_security_product: bool,
    pub ip_address: String,
    pub ecs_login_method: Option<String>,
    pub ecs_login_username: Option<String>,
    pub ecs_initial_password: Option<String>,
    pub bastion_address: Option<String>,
    pub bastion_admin_account: Option<String>,
    pub bastion_initial_password: Option<String>,
    pub physical_machine_info: Option<PhysicalMachineInfo>,
    pub cloud_vm_info: Option<CloudVirtualMachineInfo>,
    pub remarks: Option<String>,
    pub applicant: Option<String>,
    pub department: Option<String>,
    pub approver: Option<String>,
    pub approval_time: Option<DateTime<Utc>>,
    pub approval_remarks: Option<String>,
    pub rejection_reason: Option<String>,
    pub bandwidth_mbps: Option<u32>,
    pub bandwidth_type: Option<String>,
    pub public_ip_count: Option<u32>,
    pub network_type: Option<String>,
    pub project_name: Option<String>,
    pub project_code: Option<String>,
    pub business_owner: Option<String>,
    pub tech_owner: Option<String>,
    pub contact_phone: Option<String>,
    pub billing_method: Option<String>,
    pub purchase_duration: Option<u32>,
    pub cost_center: Option<String>,
    pub security_level: Option<String>,
    pub data_sensitivity: Option<String>,
    pub purpose: Option<String>,
    pub expected_delivery_time: Option<DateTime<Utc>>,
    pub application_status: Option<String>,
    pub delivery_status: Option<String>,
    pub delivery_confirmed_at: Option<DateTime<Utc>>,
    pub delivery_confirmed_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateBusinessResourceRequest {
    pub resource_type: Option<String>,
    pub ecs_name: Option<String>,
    pub ecs_status: Option<String>,
    pub resource_id: Option<String>,
    pub cloud_region: Option<String>,
    pub cloud_category: Option<String>,
    pub cloud_provider_config_id: Option<i32>,
    pub machine_room_id: Option<i32>,
    pub cabinet_id: Option<i32>,
    pub cabinet_name: Option<String>,
    pub chassis_name: Option<String>,
    pub chassis_slot: Option<String>,
    pub rack_start_u: Option<i32>,
    pub rack_end_u: Option<i32>,
    pub deployment_type: Option<String>,
    pub management_ip: Option<String>,
    pub business_ip: Option<String>,
    pub network_cidr: Option<String>,
    pub gateway: Option<String>,
    pub vlan_id: Option<String>,
    pub dns_servers: Option<String>,
    pub mac_address: Option<String>,
    pub switch_name: Option<String>,
    pub switch_port: Option<String>,
    pub zone_name: Option<String>,
    pub platform_name: Option<String>,
    pub county_city: Option<String>,
    pub vdc_name: Option<String>,
    pub customer_name: Option<String>,
    pub application_name: Option<String>,
    pub contract_name: Option<String>,
    pub instance_id: Option<String>,
    pub ecs_type: Option<String>,
    pub ecs_os: Option<String>,
    pub cpu_cores: Option<u32>,
    pub memory_gb: Option<u32>,
    pub system_disk: Option<String>,
    pub system_disk_size_gb: Option<u32>,
    pub data_disk: Option<String>,
    pub completion_time: Option<DateTime<Utc>>,
    pub release_time: Option<DateTime<Utc>>,
    pub has_security_product: Option<bool>,
    pub ip_address: Option<String>,
    pub ecs_login_method: Option<String>,
    pub ecs_login_username: Option<String>,
    pub ecs_initial_password: Option<String>,
    pub bastion_address: Option<String>,
    pub bastion_admin_account: Option<String>,
    pub bastion_initial_password: Option<String>,
    pub physical_machine_info: Option<PhysicalMachineInfo>,
    pub cloud_vm_info: Option<CloudVirtualMachineInfo>,
    pub remarks: Option<String>,
    pub applicant: Option<String>,
    pub department: Option<String>,
    pub approver: Option<String>,
    pub approval_time: Option<DateTime<Utc>>,
    pub approval_remarks: Option<String>,
    pub rejection_reason: Option<String>,
    pub bandwidth_mbps: Option<u32>,
    pub bandwidth_type: Option<String>,
    pub public_ip_count: Option<u32>,
    pub network_type: Option<String>,
    pub project_name: Option<String>,
    pub project_code: Option<String>,
    pub business_owner: Option<String>,
    pub tech_owner: Option<String>,
    pub contact_phone: Option<String>,
    pub billing_method: Option<String>,
    pub purchase_duration: Option<u32>,
    pub cost_center: Option<String>,
    pub security_level: Option<String>,
    pub data_sensitivity: Option<String>,
    pub purpose: Option<String>,
    pub expected_delivery_time: Option<DateTime<Utc>>,
    pub application_status: Option<String>,
    pub delivery_status: Option<String>,
    pub delivery_confirmed_at: Option<DateTime<Utc>>,
    pub delivery_confirmed_by: Option<String>,
}

// ── Application endpoint ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationEndpoint {
    pub id: Option<i32>,
    pub business_application_id: i32,
    pub protocol: String,
    pub dest_ip: String,
    pub nat_ip: Option<String>,
    pub dest_port: String,
    pub domain: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApplicationEndpointRequest {
    pub business_application_id: i32,
    pub protocol: String,
    pub dest_ip: String,
    pub nat_ip: String,
    pub dest_port: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateApplicationEndpointRequest {
    pub business_application_id: i32,
    pub protocol: String,
    pub dest_ip: String,
    pub nat_ip: String,
    pub dest_port: String,
    pub domain: Option<String>,
}

// ── Cloud SDK integration types ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CloudPlatformKind {
    #[serde(rename = "openstack")]
    OpenStack,
    #[serde(rename = "hcs")]
    Hcs,
    #[serde(rename = "apsara")]
    Apsara,
    #[serde(rename = "tstack")]
    TStack,
    #[serde(rename = "custom")]
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudCredentials {
    pub kind: CloudPlatformKind,
    pub endpoint: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub domain: Option<String>,
    pub project_name: Option<String>,
    pub project_id: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub region: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInstance {
    pub instance_id: String,
    pub name: String,
    pub status: String,
    pub flavor: String,
    pub image_id: String,
    pub vpc_id: Option<String>,
    pub subnet_id: Option<String>,
    pub private_ip: Vec<String>,
    pub public_ip: Vec<String>,
    pub security_groups: Vec<String>,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub system_disk_gb: u32,
    pub data_disks: Vec<CloudDisk>,
    pub os_type: String,
    pub created_at: String,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub tags: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudDisk {
    pub disk_id: String,
    pub name: String,
    pub size_gb: u32,
    pub disk_type: String,
    pub attached_to: Option<String>,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudVpc {
    pub vpc_id: String,
    pub name: String,
    pub cidr: String,
    pub status: String,
    pub subnets: Vec<CloudSubnet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSubnet {
    pub subnet_id: String,
    pub name: String,
    pub cidr: String,
    pub gateway: String,
    pub available_ips: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSecurityGroup {
    pub group_id: String,
    pub name: String,
    pub description: String,
    pub rules: Vec<CloudSecurityGroupRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSecurityGroupRule {
    pub direction: String,
    pub protocol: String,
    pub port_range: String,
    pub remote: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudRegion {
    pub region_id: String,
    pub region_name: String,
    pub zones: Vec<CloudAz>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudAz {
    pub zone_id: String,
    pub zone_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncResult {
    pub platform: CloudPlatformKind,
    pub instances_synced: u32,
    pub networks_synced: u32,
    pub errors: Vec<String>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncRequest {
    pub config_id: i32,
    pub sync_instances: bool,
    pub sync_networks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudTestResult {
    pub success: bool,
    pub message: String,
    pub response_time_ms: Option<u64>,
    pub tested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInstanceActionRequest {
    pub config_id: i32,
    pub instance_id: String,
    pub action: CloudInstanceAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CloudInstanceAction {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "reboot")]
    Reboot,
    #[serde(rename = "terminate")]
    Terminate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudCreateInstanceRequest {
    pub config_id: i32,
    pub name: String,
    pub image_id: String,
    pub flavor: String,
    pub vpc_id: Option<String>,
    pub subnet_id: Option<String>,
    pub security_group_ids: Vec<String>,
    pub system_disk_gb: u32,
    pub data_disks: Vec<CloudDisk>,
    pub count: u32,
    pub user_data: Option<String>,
}
