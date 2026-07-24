//! Huawei Cloud Stack (HCS) adapter.
//!
//! HCS is Huawei's on-premise private cloud offering, compatible with
//! standard Huawei Cloud IAM / ECS / VPC / EVS APIs.
//! Auth: AK/SK signing (same as public Huawei Cloud)
//! Endpoint: http(s)://<hcs-master>/v2.1 or similar.

use crate::cloud_adapter::CloudApi;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_scan_api::*;
use serde_json::Value;

pub struct HcsAdapter;

impl HcsAdapter {
    pub fn new() -> Self {
        Self
    }

    fn sign_request(&self, _ak: &str, _sk: &str, _method: &str, _uri: &str, _body: &str) -> String {
        // HCS uses HW Cloud AK/SK HMAC-SHA256 signing.
        // Simplified implementation for the adapter interface.
        "SDK-HMAC-SHA256 Credential=placeholder".into()
    }
}

#[async_trait]
impl CloudApi for HcsAdapter {
    fn kind(&self) -> CloudPlatformKind {
        CloudPlatformKind::Hcs
    }

    async fn test_connection(&self, creds: &CloudCredentials) -> Result<CloudTestResult, String> {
        let start = std::time::Instant::now();
        let client = reqwest::Client::new();
        let result = client
            .get(format!(
                "{}/v1/{}/cloudservers",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .header("X-Auth-Token", creds.token.clone().unwrap_or_default())
            .send()
            .await;
        Ok(match result {
            Ok(r) => CloudTestResult {
                success: r.status().is_success(),
                message: format!("HCS 响应: {}", r.status()),
                response_time_ms: Some(start.elapsed().as_millis() as u64),
                tested_at: Utc::now(),
            },
            Err(e) => CloudTestResult {
                success: false,
                message: format!("HCS 连接失败: {e}"),
                response_time_ms: None,
                tested_at: Utc::now(),
            },
        })
    }

    async fn list_instances(&self, creds: &CloudCredentials) -> Result<Vec<CloudInstance>, String> {
        let client = reqwest::Client::new();
        let endpoint = format!(
            "{}/v1/{}/cloudservers/detail",
            creds.endpoint.trim_end_matches('/'),
            creds.project_id.clone().unwrap_or_default()
        );
        let resp: Value = client
            .get(&endpoint)
            .header("X-Auth-Token", creds.token.clone().unwrap_or_default())
            .send()
            .await
            .map_err(|e| format!("HCS ECS API: {e}"))?
            .json()
            .await
            .map_err(|e| format!("HCS ECS parse: {e}"))?;
        Ok(resp["servers"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|s| CloudInstance {
                instance_id: s["id"].as_str().unwrap_or("").into(),
                name: s["name"].as_str().unwrap_or("").into(),
                status: s["status"].as_str().unwrap_or("UNKNOWN").into(),
                flavor: s["flavor"]["id"].as_str().unwrap_or("").into(),
                image_id: s["image"]["id"].as_str().unwrap_or("").into(),
                vpc_id: s["metadata"]["vpc_id"].as_str().map(String::from),
                subnet_id: None,
                private_ip: s["addresses"].as_object().map_or(vec![], |o| {
                    o.values()
                        .flat_map(|ips| ips.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter())
                        .filter_map(|ip| ip["addr"].as_str().map(String::from))
                        .collect()
                }),
                public_ip: s
                    .get("accessIPv4")
                    .and_then(|v| v.as_str())
                    .map(|v| vec![v.into()])
                    .unwrap_or_default(),
                security_groups: s.get("security_groups").and_then(|v| v.as_array()).map_or(
                    vec![],
                    |gs| {
                        gs.iter()
                            .filter_map(|g| g["name"].as_str().map(String::from))
                            .collect()
                    },
                ),
                cpu_cores: s["flavor"]["vcpus"].as_u64().unwrap_or(0) as u32,
                memory_mb: (s["flavor"]["ram"].as_u64().unwrap_or(0)) as u32,
                system_disk_gb: s
                    .get("root_volume_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                data_disks: s
                    .get("data_volumes")
                    .and_then(|v| v.as_array())
                    .map_or(vec![], |ds| {
                        ds.iter()
                            .map(|d| CloudDisk {
                                disk_id: d["id"].as_str().unwrap_or("").into(),
                                name: d
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("data")
                                    .into(),
                                size_gb: d["size"].as_u64().unwrap_or(0) as u32,
                                disk_type: d["volume_type"].as_str().unwrap_or("SATA").into(),
                                attached_to: Some(s["id"].as_str().unwrap_or("").into()),
                                encrypted: d
                                    .get("metadata")
                                    .and_then(|m| m["__system__encrypted"].as_str())
                                    .map(|v| v == "1")
                                    .unwrap_or(false),
                            })
                            .collect()
                    }),
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
            })
            .collect())
    }

    async fn get_instance(
        &self,
        creds: &CloudCredentials,
        id: &str,
    ) -> Result<CloudInstance, String> {
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v1/{}/cloudservers/{}",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default(),
                id
            ))
            .header("X-Auth-Token", creds.token.clone().unwrap_or_default())
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?
            .json()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
        let s = &resp["server"];
        Ok(CloudInstance {
            instance_id: id.into(),
            name: s["name"].as_str().unwrap_or("").into(),
            status: s["status"].as_str().unwrap_or("").into(),
            flavor: "".into(),
            image_id: "".into(),
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
            created_at: "".into(),
            region: creds.region.clone(),
            zone: None,
            tags: vec![],
        })
    }

    async fn start_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        reqwest::Client::new()
            .post(format!(
                "{}/v1/{}/cloudservers/action",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .json(&serde_json::json!({"os-start": {"servers": [{"id": id}]}}))
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
        Ok(())
    }
    async fn stop_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        reqwest::Client::new()
            .post(format!(
                "{}/v1/{}/cloudservers/action",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .json(&serde_json::json!({"os-stop": {"servers": [{"id": id}]}}))
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
        Ok(())
    }
    async fn reboot_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        reqwest::Client::new()
            .post(format!(
                "{}/v1/{}/cloudservers/{}/action",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default(),
                id
            ))
            .json(&serde_json::json!({"reboot": {"type": "SOFT"}}))
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
        Ok(())
    }
    async fn terminate_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        reqwest::Client::new()
            .delete(format!(
                "{}/v1/{}/cloudservers/{}",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default(),
                id
            ))
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
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
        let body = serde_json::json!({"server": {"name": name, "imageRef": image_id, "flavorRef": flavor, "root_volume": {"volumetype": "SSD", "size": system_disk_gb}, "count": 1}});
        let client = reqwest::Client::new();
        let resp: Value = client
            .post(format!(
                "{}/v1/{}/cloudservers",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?
            .json()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
        let id = resp["server"]["id"].as_str().unwrap_or("").into();
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
            created_at: Utc::now().to_rfc3339(),
            region: creds.region.clone(),
            zone: None,
            tags: vec![],
        })
    }
    async fn list_networks(&self, creds: &CloudCredentials) -> Result<Vec<CloudVpc>, String> {
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v1/{}/vpcs",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .send()
            .await
            .map_err(|e| format!("HCS VPC: {e}"))?
            .json()
            .await
            .map_err(|e| format!("HCS VPC: {e}"))?;
        Ok(resp["vpcs"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|v| CloudVpc {
                vpc_id: v["id"].as_str().unwrap_or("").into(),
                name: v["name"].as_str().unwrap_or("").into(),
                cidr: v["cidr"].as_str().unwrap_or("0.0.0.0/0").into(),
                status: v["status"].as_str().unwrap_or("OK").into(),
                subnets: vec![],
            })
            .collect())
    }
    async fn list_security_groups(
        &self,
        creds: &CloudCredentials,
    ) -> Result<Vec<CloudSecurityGroup>, String> {
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v1/{}/security-groups",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?
            .json()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
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
                        port_range: r
                            .get("port_range_max")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0")
                            .into(),
                        remote: r
                            .get("remote_ip_prefix")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0.0.0.0/0")
                            .into(),
                        action: "allow".into(),
                    })
                    .collect(),
            })
            .collect())
    }
    async fn list_regions(&self, creds: &CloudCredentials) -> Result<Vec<CloudRegion>, String> {
        let client = reqwest::Client::new();
        let resp: Value = client
            .get(format!(
                "{}/v1/{}/os-availability-zone",
                creds.endpoint.trim_end_matches('/'),
                creds.project_id.clone().unwrap_or_default()
            ))
            .send()
            .await
            .map_err(|e| format!("HCS: {e}"))?
            .json()
            .await
            .map_err(|e| format!("HCS: {e}"))?;
        let zones: Vec<CloudAz> = resp["availabilityZoneInfo"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|z| CloudAz {
                zone_id: z["zoneName"].as_str().unwrap_or("").into(),
                zone_name: z["zoneName"].as_str().unwrap_or("").into(),
            })
            .collect();
        Ok(vec![CloudRegion {
            region_id: creds.region.clone().unwrap_or_default(),
            region_name: creds.region.clone().unwrap_or_default(),
            zones,
        }])
    }
}

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
    let client = reqwest::Client::new();
    let resp: Value = client
        .get(format!(
            "{}/v3/projects",
            cfg.region_id.trim_end_matches('/')
        ))
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
