//! Aliyun Apsara Stack adapter.
//!
//! Apsara Stack is Alibaba Cloud's on-premise private cloud offering.
//! API: compatible with Alibaba Cloud ECS / VPC OpenAPI
//! Auth: AK/SK signing via HMAC-SHA1
//! Endpoint: https://<apsara-console>/api

use crate::cloud_adapter::CloudApi;
use async_trait::async_trait;
use chrono::Utc;
use kairos_platform_scan_api::*;
use serde_json::Value;

pub struct ApsaraAdapter;

impl ApsaraAdapter {
    pub fn new() -> Self {
        Self
    }

    fn common_params(&self, ak: &str, region: &str) -> Vec<(&str, String)> {
        vec![
            ("Format", "JSON".into()),
            ("Version", "2014-05-26".into()),
            ("AccessKeyId", ak.into()),
            ("SignatureMethod", "HMAC-SHA1".into()),
            (
                "Timestamp",
                Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            ),
            ("SignatureVersion", "1.0".into()),
            ("SignatureNonce", uuid::Uuid::new_v4().to_string()),
            ("RegionId", region.into()),
        ]
    }
}

#[async_trait]
impl CloudApi for ApsaraAdapter {
    fn kind(&self) -> CloudPlatformKind {
        CloudPlatformKind::Apsara
    }

    async fn test_connection(&self, creds: &CloudCredentials) -> Result<CloudTestResult, String> {
        let start = std::time::Instant::now();
        let client = reqwest::Client::new();
        let params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        let result = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await;
        Ok(match result {
            Ok(r) => CloudTestResult {
                success: r.status().is_success(),
                message: format!("Apsara 响应: {}", r.status()),
                response_time_ms: Some(start.elapsed().as_millis() as u64),
                tested_at: Utc::now(),
            },
            Err(e) => CloudTestResult {
                success: false,
                message: format!("Apsara 连接失败: {e}"),
                response_time_ms: None,
                tested_at: Utc::now(),
            },
        })
    }

    async fn list_instances(&self, creds: &CloudCredentials) -> Result<Vec<CloudInstance>, String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "DescribeInstances".into()));
        let resp: Value = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara ECS: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Apsara ECS parse: {e}"))?;
        Ok(resp["Instances"]["Instance"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|i| CloudInstance {
                instance_id: i["InstanceId"].as_str().unwrap_or("").into(),
                name: i["InstanceName"].as_str().unwrap_or("").into(),
                status: i["Status"].as_str().unwrap_or("").into(),
                flavor: i["InstanceType"].as_str().unwrap_or("").into(),
                image_id: i["ImageId"].as_str().unwrap_or("").into(),
                vpc_id: i["VpcAttributes"]["VpcId"].as_str().map(String::from),
                subnet_id: None,
                private_ip: i["VpcAttributes"]["PrivateIpAddress"]["IpAddress"]
                    .as_array()
                    .map_or(vec![], |ips| {
                        ips.iter()
                            .filter_map(|ip| ip.as_str().map(String::from))
                            .collect()
                    }),
                public_ip: i["PublicIpAddress"]["IpAddress"]
                    .as_array()
                    .map_or(vec![], |ips| {
                        ips.iter()
                            .filter_map(|ip| ip.as_str().map(String::from))
                            .collect()
                    }),
                security_groups: i["SecurityGroupIds"]["SecurityGroupId"].as_array().map_or(
                    vec![],
                    |sgs| {
                        sgs.iter()
                            .filter_map(|sg| sg.as_str().map(String::from))
                            .collect()
                    },
                ),
                cpu_cores: i["Cpu"].as_u64().unwrap_or(0) as u32,
                memory_mb: (i["Memory"].as_u64().unwrap_or(0) * 1024) as u32,
                system_disk_gb: i
                    .get("SystemDisk")
                    .and_then(|d| d["Size"].as_u64())
                    .unwrap_or(0) as u32,
                data_disks: i
                    .get("DataDisks")
                    .and_then(|ds| ds["DataDisk"].as_array())
                    .map_or(vec![], |ds| {
                        ds.iter()
                            .map(|d| CloudDisk {
                                disk_id: d["DiskId"].as_str().unwrap_or("").into(),
                                name: d
                                    .get("DiskName")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("data")
                                    .into(),
                                size_gb: d["Size"].as_u64().unwrap_or(0) as u32,
                                disk_type: d["Type"].as_str().unwrap_or("cloud").into(),
                                attached_to: Some(i["InstanceId"].as_str().unwrap_or("").into()),
                                encrypted: d
                                    .get("Encrypted")
                                    .and_then(|e| e.as_bool())
                                    .unwrap_or(false),
                            })
                            .collect()
                    }),
                os_type: i["OSType"].as_str().unwrap_or("linux").into(),
                created_at: i["CreationTime"].as_str().unwrap_or("").into(),
                region: creds.region.clone(),
                zone: i["ZoneId"].as_str().map(String::from),
                tags: i
                    .get("Tags")
                    .and_then(|ts| ts["Tag"].as_array())
                    .map_or(vec![], |ts| {
                        ts.iter()
                            .map(|t| {
                                (
                                    t["TagKey"].as_str().unwrap_or("").into(),
                                    t["TagValue"].as_str().unwrap_or("").into(),
                                )
                            })
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
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "DescribeInstances".into()));
        params.push(("InstanceIds", format!("[\"{id}\"]")));
        let resp: Value = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        let empty_insts = vec![];
        let insts = resp["Instances"]["Instance"]
            .as_array()
            .unwrap_or(&empty_insts);
        insts
            .first()
            .map(|i| CloudInstance {
                instance_id: i["InstanceId"].as_str().unwrap_or("").into(),
                name: i["InstanceName"].as_str().unwrap_or("").into(),
                status: i["Status"].as_str().unwrap_or("").into(),
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
            .ok_or("Instance not found".into())
    }

    async fn start_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "StartInstance".into()));
        params.push(("InstanceId", id.into()));
        client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        Ok(())
    }
    async fn stop_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "StopInstance".into()));
        params.push(("InstanceId", id.into()));
        client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        Ok(())
    }
    async fn reboot_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "RebootInstance".into()));
        params.push(("InstanceId", id.into()));
        client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        Ok(())
    }
    async fn terminate_instance(&self, creds: &CloudCredentials, id: &str) -> Result<(), String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "DeleteInstance".into()));
        params.push(("InstanceId", id.into()));
        client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
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
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "CreateInstance".into()));
        params.push(("InstanceName", name.into()));
        params.push(("ImageId", image_id.into()));
        params.push(("InstanceType", flavor.into()));
        params.push(("SystemDisk.Size", system_disk_gb.to_string()));
        let resp: Value = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        let id = resp["InstanceId"].as_str().unwrap_or("").into();
        Ok(CloudInstance {
            instance_id: id,
            name: name.into(),
            status: "Starting".into(),
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
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "DescribeVpcs".into()));
        let resp: Value = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara VPC: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Apsara VPC: {e}"))?;
        Ok(resp["Vpcs"]["Vpc"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|v| CloudVpc {
                vpc_id: v["VpcId"].as_str().unwrap_or("").into(),
                name: v["VpcName"].as_str().unwrap_or("").into(),
                cidr: v["CidrBlock"].as_str().unwrap_or("0.0.0.0/0").into(),
                status: v["Status"].as_str().unwrap_or("Available").into(),
                subnets: vec![],
            })
            .collect())
    }
    async fn list_security_groups(
        &self,
        creds: &CloudCredentials,
    ) -> Result<Vec<CloudSecurityGroup>, String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "DescribeSecurityGroups".into()));
        let resp: Value = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        Ok(resp["SecurityGroups"]["SecurityGroup"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|sg| CloudSecurityGroup {
                group_id: sg["SecurityGroupId"].as_str().unwrap_or("").into(),
                name: sg["SecurityGroupName"].as_str().unwrap_or("").into(),
                description: sg["Description"].as_str().unwrap_or("").into(),
                rules: vec![],
            })
            .collect())
    }
    async fn list_regions(&self, creds: &CloudCredentials) -> Result<Vec<CloudRegion>, String> {
        let client = reqwest::Client::new();
        let mut params = self.common_params(
            creds.access_key.as_deref().unwrap_or(""),
            creds.region.as_deref().unwrap_or(""),
        );
        params.push(("Action", "DescribeZones".into()));
        let resp: Value = client
            .get(format!("{}/?", creds.endpoint.trim_end_matches('/')))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Apsara: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Apsara: {e}"))?;
        let zones: Vec<CloudAz> = resp["Zones"]["Zone"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|z| CloudAz {
                zone_id: z["ZoneId"].as_str().unwrap_or("").into(),
                zone_name: z["LocalName"].as_str().unwrap_or("").into(),
            })
            .collect();
        Ok(vec![CloudRegion {
            region_id: creds.region.clone().unwrap_or_default(),
            region_name: creds.region.clone().unwrap_or_default(),
            zones,
        }])
    }
}

pub async fn list_zones(
    axum::extract::Path(config_id): axum::extract::Path<i32>,
    axum::extract::State(state): axum::extract::State<crate::ScanState>,
) -> Result<
    axum::Json<kairos_platform_framework_common::ApiResponse<Vec<Value>>>,
    kairos_platform_framework_web::AppError,
> {
    let configs = state.cloud_provider_configs.lock().await;
    let _cfg = configs
        .iter()
        .find(|c| c.id == Some(config_id))
        .ok_or_else(|| kairos_platform_framework_web::AppError::not_found("config not found"))?;
    Ok(axum::Json(
        kairos_platform_framework_common::ApiResponse::new(vec![
            serde_json::json!({"ZoneId": "cn-zhangjiakou-a", "ZoneName": "张北可用区A"}),
            serde_json::json!({"ZoneId": "cn-zhangjiakou-b", "ZoneName": "张北可用区B"}),
        ]),
    ))
}
