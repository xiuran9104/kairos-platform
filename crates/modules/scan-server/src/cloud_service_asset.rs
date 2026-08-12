use crate::ScanState;
use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_web::AppError;
use kairos_platform_scan_api::CloudServiceAsset;

pub async fn list(
    State(s): State<ScanState>,
) -> Result<Json<ApiResponse<Vec<CloudServiceAsset>>>, AppError> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>("SELECT COALESCE(jsonb_agg(to_jsonb(t)), '[]'::jsonb) FROM (SELECT r.id::text,c.id::integer as provider_config_id,c.provider,c.account_name,r.resource_type as asset_type,'cloud_sync' as source_type,r.ecs_name as name,r.instance_id,r.ecs_status as status,r.cloud_region as region,c.region_id,r.cloud_category as availability_zone,p.platform_name as cloud_platform,z.zone_name as cloud_zone,r.ecs_type as instance_type,r.cpu_cores,r.memory_gb,r.system_disk as system_disk_type,r.system_disk_size_gb,'Linux' as os_type,r.ecs_os as os_name,r.ip_address,NULL::text as public_ip,r.customer_name,NULL::text as department,NULL::text as project,NULL::text as application_name,NULL::text as owner_name,r.created_at::text,r.updated_at::text FROM scan_business_resources r JOIN scan_cloud_provider_configs c ON c.id=r.cloud_provider_config_id JOIN scan_cloud_platforms p ON p.id=c.platform_id JOIN scan_cloud_zones z ON z.id=p.zone_id WHERE r.deleted=0 AND c.deleted=0 ORDER BY r.id DESC) t")
        .fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.unwrap_or_default();
    Ok(Json(ApiResponse::new(
        serde_json::from_value::<Vec<CloudServiceAsset>>(rows).unwrap_or_default(),
    )))
}
pub async fn get_one(
    State(s): State<ScanState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<CloudServiceAsset>>, AppError> {
    let id_num: i32 = id
        .strip_prefix("br-")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let v = sqlx::query_scalar::<_, serde_json::Value>("SELECT to_jsonb(t) FROM (SELECT r.id::text,c.id::integer as provider_config_id,c.provider,c.account_name,r.resource_type as asset_type,'cloud_sync' as source_type,r.ecs_name as name,r.instance_id,r.ecs_status as status,r.cloud_region as region,c.region_id,r.cloud_category as availability_zone,p.platform_name as cloud_platform,z.zone_name as cloud_zone,r.ecs_type as instance_type,r.cpu_cores,r.memory_gb,r.system_disk as system_disk_type,r.system_disk_size_gb,'Linux' as os_type,r.ecs_os as os_name,r.ip_address,NULL::text as public_ip,r.customer_name,NULL::text as department,NULL::text as project,NULL::text as application_name,NULL::text as owner_name,r.created_at::text,r.updated_at::text FROM scan_business_resources r JOIN scan_cloud_provider_configs c ON c.id=r.cloud_provider_config_id JOIN scan_cloud_platforms p ON p.id=c.platform_id JOIN scan_cloud_zones z ON z.id=p.zone_id WHERE r.id=$1 AND r.deleted=0 AND c.deleted=0) t")
        .bind(id_num).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
