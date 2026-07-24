//! OpenStack adapter — Keystone v3 + Nova + Neutron + Cinder v2/v3.
//!
//! REST API reference: https://docs.openstack.org/api-ref/

use async_trait::async_trait;
use kairos_platform_scan_api::*;
use serde_json::Value;

use crate::cloud_adapter::CloudApi;

pub struct OpenStackAdapter;

impl OpenStackAdapter {
    pub fn new() -> Self {
        Self
    }

    async fn keystone_token(&self, creds: &CloudCredentials) -> Result<String, String> {
        let body = serde_json::json!({
            "auth": {
                "identity": {
                    "methods": ["password"],
                    "password": {
                        "user": {
                            "name": creds.username.clone().unwrap_or_default(),
                            "domain": { "name": creds.domain.clone().unwrap_or_else(|| "Default".into()) },
                            "password": creds.password.clone().unwrap_or_default()
                        }
                    }
                },
                "scope": {
                    "project": {
                        "name": creds.project_name.clone().unwrap_or_else(|| "admin".into()),
                        "domain": { "name": creds.domain.clone().unwrap_or_else(|| "Default".into()) }
                    }
                }
            }
        });
        let client = reqwest::Client::new();
        let resp = client
            .post(format!(
                "{}/v3/auth/tokens",
                creds.endpoint.trim_end_matches('/')
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Keystone auth failed: {e}"))?;
        resp.headers()
            .get("x-subject-token")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or_else(|| "No token in response".into())
    }

    fn auth_header(token: &str) -> String {
        format!("X-Auth-Token: {token}")
    }
}

#[async_trait]
impl CloudApi for OpenStackAdapter {
    fn kind(&self) -> CloudPlatformKind {
        CloudPlatformKind::OpenStack
    }

    async fn test_connection(&self, creds: &CloudCredentials) -> Result<CloudTestResult, String> {
        let start = std::time::Instant::now();
        match self.keystone_token(creds).await {
            Ok(_) => Ok(CloudTestResult {
                success: true,
                message: "OpenStack Keystone 认证成功".into(),
                response_time_ms: Some(start.elapsed().as_millis() as u64),
                tested_at: chrono::Utc::now(),
            }),
            Err(e) => Ok(CloudTestResult {
                success: false,
                message: e,
                response_time_ms: None,
                tested_at: chrono::Utc::now(),
            }),
        }
    }

    async fn list_instances(&self, creds: &CloudCredentials) -> Result<Vec<CloudInstance>, String> {
        let token = self.keystone_token(creds).await?;
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v2.1/servers/detail",
                creds.endpoint.trim_end_matches('/')
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| format!("Nova API: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Nova parse: {e}"))?;

        let empty_arr = vec![];
        let servers = resp["servers"].as_array().unwrap_or(&empty_arr);
        Ok(servers
            .iter()
            .map(|s| {
                let addrs = &s["addresses"];
                let priv_ips: Vec<String> = addrs.as_object().map_or(vec![], |o| {
                    o.values()
                        .flat_map(|net| net.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter())
                        .filter_map(|a| a["addr"].as_str().map(String::from))
                        .collect()
                });
                CloudInstance {
                    instance_id: s["id"].as_str().unwrap_or("").into(),
                    name: s["name"].as_str().unwrap_or("").into(),
                    status: s["status"].as_str().unwrap_or("UNKNOWN").into(),
                    flavor: s["flavor"]["id"].as_str().unwrap_or("").into(),
                    image_id: s["image"]["id"].as_str().unwrap_or("").into(),
                    vpc_id: None,
                    subnet_id: None,
                    private_ip: priv_ips.clone(),
                    public_ip: vec![],
                    security_groups: s["security_groups"].as_array().map_or(vec![], |gs| {
                        gs.iter()
                            .filter_map(|g| g["name"].as_str().map(String::from))
                            .collect()
                    }),
                    cpu_cores: 0,
                    memory_mb: 0,
                    system_disk_gb: 0,
                    data_disks: vec![],
                    os_type: s
                        .get("metadata")
                        .and_then(|m| m["os_type"].as_str())
                        .unwrap_or("linux")
                        .into(),
                    created_at: s["created"].as_str().unwrap_or("").into(),
                    region: creds.region.clone(),
                    zone: s
                        .get("OS-EXT-AZ:availability_zone")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    tags: s
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map_or(vec![], |ts| {
                            ts.iter()
                                .map(|t| (t.as_str().unwrap_or("").into(), "".into()))
                                .collect()
                        }),
                }
            })
            .collect())
    }

    async fn get_instance(
        &self,
        creds: &CloudCredentials,
        id: &str,
    ) -> Result<CloudInstance, String> {
        let token = self.keystone_token(creds).await?;
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v2.1/servers/{}",
                creds.endpoint.trim_end_matches('/'),
                id
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| format!("Nova API: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Nova parse: {e}"))?;
        let s = &resp["server"];
        Ok(CloudInstance {
            instance_id: s["id"].as_str().unwrap_or("").into(),
            name: s["name"].as_str().unwrap_or("").into(),
            status: s["status"].as_str().unwrap_or("UNKNOWN").into(),
            flavor: s["flavor"]["id"].as_str().unwrap_or("").into(),
            image_id: s["image"]["id"].as_str().unwrap_or("").into(),
            vpc_id: None,
            subnet_id: None,
            private_ip: vec![],
            public_ip: vec![],
            security_groups: vec![],
            cpu_cores: 0,
            memory_mb: 0,
            system_disk_gb: 0,
            data_disks: vec![],
            os_type: "linux".into(),
            created_at: s["created"].as_str().unwrap_or("").into(),
            region: creds.region.clone(),
            zone: None,
            tags: vec![],
        })
    }

    async fn start_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let token = self.keystone_token(creds).await?;
        reqwest::Client::new()
            .post(format!(
                "{}/v2.1/servers/{}/action",
                creds.endpoint.trim_end_matches('/'),
                id
            ))
            .header("X-Auth-Token", &token)
            .json(&serde_json::json!({"os-start": null}))
            .send()
            .await
            .map_err(|e| format!("Nova start: {e}"))?;
        Ok(())
    }

    async fn stop_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let token = self.keystone_token(creds).await?;
        reqwest::Client::new()
            .post(format!(
                "{}/v2.1/servers/{}/action",
                creds.endpoint.trim_end_matches('/'),
                id
            ))
            .header("X-Auth-Token", &token)
            .json(&serde_json::json!({"os-stop": null}))
            .send()
            .await
            .map_err(|e| format!("Nova stop: {e}"))?;
        Ok(())
    }

    async fn reboot_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let token = self.keystone_token(creds).await?;
        reqwest::Client::new()
            .post(format!(
                "{}/v2.1/servers/{}/action",
                creds.endpoint.trim_end_matches('/'),
                id
            ))
            .header("X-Auth-Token", &token)
            .json(&serde_json::json!({"reboot": {"type": "SOFT"}}))
            .send()
            .await
            .map_err(|e| format!("Nova reboot: {e}"))?;
        Ok(())
    }

    async fn terminate_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let token = self.keystone_token(creds).await?;
        reqwest::Client::new()
            .delete(format!(
                "{}/v2.1/servers/{}",
                creds.endpoint.trim_end_matches('/'),
                id
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| format!("Nova delete: {e}"))?;
        Ok(())
    }

    async fn create_instance(
        &self,
        creds: &CloudCredentials,
        name: &str,
        image_id: &str,
        flavor: &str,
        system_disk_gb: u32,
    ) -> Result<CloudInstance, String> {
        let token = self.keystone_token(creds).await?;
        let body = serde_json::json!({
            "server": {
                "name": name,
                "imageRef": image_id,
                "flavorRef": flavor,
                "networks": "auto",
                "block_device_mapping_v2": [{"boot_index": 0, "uuid": image_id, "source_type": "image", "destination_type": "volume", "volume_size": system_disk_gb, "delete_on_termination": true}]
            }
        });
        let client = reqwest::Client::new();
        let resp: Value = client
            .post(format!(
                "{}/v2.1/servers",
                creds.endpoint.trim_end_matches('/')
            ))
            .header("X-Auth-Token", &token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Nova create: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Nova parse: {e}"))?;
        let id = resp["server"]["id"].as_str().unwrap_or("").to_string();
        Ok(CloudInstance {
            instance_id: id,
            name: name.into(),
            status: "BUILD".into(),
            flavor: flavor.into(),
            image_id: image_id.into(),
            vpc_id: None,
            subnet_id: None,
            private_ip: vec![],
            public_ip: vec![],
            security_groups: vec![],
            cpu_cores: 0,
            memory_mb: 0,
            system_disk_gb,
            data_disks: vec![],
            os_type: "linux".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            region: creds.region.clone(),
            zone: None,
            tags: vec![],
        })
    }

    async fn list_networks(&self, creds: &CloudCredentials) -> Result<Vec<CloudVpc>, String> {
        let token = self.keystone_token(creds).await?;
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v2.0/networks",
                creds.endpoint.trim_end_matches('/')
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| format!("Neutron API: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Neutron parse: {e}"))?;
        Ok(resp["networks"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|n| CloudVpc {
                vpc_id: n["id"].as_str().unwrap_or("").into(),
                name: n["name"].as_str().unwrap_or("").into(),
                cidr: "0.0.0.0/0".into(),
                status: n["status"].as_str().unwrap_or("ACTIVE").into(),
                subnets: vec![],
            })
            .collect())
    }

    async fn list_security_groups(
        &self,
        creds: &CloudCredentials,
    ) -> Result<Vec<CloudSecurityGroup>, String> {
        let token = self.keystone_token(creds).await?;
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v2.0/security-groups",
                creds.endpoint.trim_end_matches('/')
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| format!("Neutron API: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Neutron parse: {e}"))?;
        Ok(resp["security_groups"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|sg| CloudSecurityGroup {
                group_id: sg["id"].as_str().unwrap_or("").into(),
                name: sg["name"].as_str().unwrap_or("").into(),
                description: sg["description"].as_str().unwrap_or("").into(),
                rules: sg["security_group_rules"]
                    .as_array()
                    .map(|a| a.as_slice())
                    .unwrap_or(&[])
                    .iter()
                    .map(|r| CloudSecurityGroupRule {
                        direction: r["direction"].as_str().unwrap_or("ingress").into(),
                        protocol: r["protocol"].as_str().unwrap_or("tcp").into(),
                        port_range: format!("{}", r["port_range_max"].as_u64().unwrap_or(0)),
                        remote: r["remote_ip_prefix"].as_str().unwrap_or("0.0.0.0/0").into(),
                        action: "allow".into(),
                    })
                    .collect(),
            })
            .collect())
    }

    async fn list_regions(&self, creds: &CloudCredentials) -> Result<Vec<CloudRegion>, String> {
        let token = self.keystone_token(creds).await?;
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v3/regions",
                creds.endpoint.trim_end_matches('/')
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| format!("Keystone API: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Keystone parse: {e}"))?;
        // OpenStack also has compute availability zones
        let az_resp: Value = client
            .get(format!(
                "{}/v2.1/os-availability-zone",
                creds.endpoint.trim_end_matches('/')
            ))
            .header("X-Auth-Token", &token)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .unwrap_or_default();
        let zones: Vec<CloudAz> = az_resp["availabilityZoneInfo"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter(|z| z["zoneState"]["available"].as_bool().unwrap_or(false))
            .map(|z| CloudAz {
                zone_id: z["zoneName"].as_str().unwrap_or("").into(),
                zone_name: z["zoneName"].as_str().unwrap_or("").into(),
            })
            .collect();
        Ok(resp["regions"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|r| CloudRegion {
                region_id: r["id"].as_str().unwrap_or("").into(),
                region_name: r
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into(),
                zones: zones.clone(),
            })
            .collect())
    }
}

// ── OpenStack-specific endpoints ───────────────────────────────────────

pub async fn list_projects(
    axum::extract::Path(config_id): axum::extract::Path<i32>,
    axum::extract::State(state): axum::extract::State<crate::ScanState>,
) -> Result<
    axum::Json<kairos_platform_framework_common::ApiResponse<Vec<Value>>>,
    kairos_platform_framework_web::AppError,
> {
    let configs = state.cloud_provider_configs.lock().await;
    let cfg = configs
        .iter()
        .find(|c| c.id == Some(config_id))
        .ok_or_else(|| kairos_platform_framework_web::AppError::not_found("config not found"))?;
    let adapter = OpenStackAdapter::new();
    let creds = CloudCredentials {
        kind: CloudPlatformKind::OpenStack,
        endpoint: cfg.region_id.clone(),
        username: Some(cfg.account_name.clone()),
        password: Some(cfg.access_key_secret.clone()),
        domain: None,
        project_name: None,
        project_id: None,
        access_key: None,
        secret_key: None,
        region: Some(cfg.region_name.clone()),
        token: None,
    };
    let token = adapter
        .keystone_token(&creds)
        .await
        .map_err(|e| kairos_platform_framework_web::AppError::internal(e))?;
    let client = reqwest::Client::new();
    let resp: Value = client
        .get(format!(
            "{}/v3/projects",
            cfg.region_id.trim_end_matches('/')
        ))
        .header("X-Auth-Token", &token)
        .send()
        .await
        .map_err(|e| kairos_platform_framework_web::AppError::internal(e.to_string()))?
        .json()
        .await
        .unwrap_or_default();
    Ok(axum::Json(
        kairos_platform_framework_common::ApiResponse::new(
            resp["projects"].as_array().cloned().unwrap_or_default(),
        ),
    ))
}

pub async fn list_flavors(
    axum::extract::Path(config_id): axum::extract::Path<i32>,
    axum::extract::State(state): axum::extract::State<crate::ScanState>,
) -> Result<
    axum::Json<kairos_platform_framework_common::ApiResponse<Vec<Value>>>,
    kairos_platform_framework_web::AppError,
> {
    let configs = state.cloud_provider_configs.lock().await;
    let cfg = configs
        .iter()
        .find(|c| c.id == Some(config_id))
        .ok_or_else(|| kairos_platform_framework_web::AppError::not_found("config not found"))?;
    let adapter = OpenStackAdapter::new();
    let creds = CloudCredentials {
        kind: CloudPlatformKind::OpenStack,
        endpoint: cfg.region_id.clone(),
        username: Some(cfg.account_name.clone()),
        password: Some(cfg.access_key_secret.clone()),
        domain: None,
        project_name: None,
        project_id: None,
        access_key: None,
        secret_key: None,
        region: Some(cfg.region_name.clone()),
        token: None,
    };
    let token = adapter
        .keystone_token(&creds)
        .await
        .map_err(|e| kairos_platform_framework_web::AppError::internal(e))?;
    let client = reqwest::Client::new();
    let resp: Value = client
        .get(format!(
            "{}/v2.1/flavors/detail",
            cfg.region_id.trim_end_matches('/')
        ))
        .header("X-Auth-Token", &token)
        .send()
        .await
        .map_err(|e| kairos_platform_framework_web::AppError::internal(e.to_string()))?
        .json()
        .await
        .unwrap_or_default();
    Ok(axum::Json(
        kairos_platform_framework_common::ApiResponse::new(
            resp["flavors"].as_array().cloned().unwrap_or_default(),
        ),
    ))
}
