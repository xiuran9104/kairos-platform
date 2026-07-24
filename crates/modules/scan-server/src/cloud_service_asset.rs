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
    let rows = sqlx::query_scalar::<_, serde_json::Value>("SELECT jsonb_agg(to_jsonb(t)) FROM (SELECT id::text,resource_type as asset_type,'business_resource' as source_type,ecs_name as name,instance_id,ecs_status as status,county_city as region,cloud_category as cloud_platform,COALESCE(zone_name,'') as cloud_zone,ecs_type as instance_type,cpu_cores,memory_gb,system_disk as system_disk_type,system_disk_size_gb,'Linux' as os_type,ecs_os as os_name,ip_address,NULL as public_ip,customer_name,application_name,created_at::text,updated_at::text FROM scan_business_resources WHERE deleted=0 ORDER BY id DESC) t")
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
    let v = sqlx::query_scalar::<_, serde_json::Value>("SELECT to_jsonb(t) FROM (SELECT id::text,resource_type as asset_type,'business_resource' as source_type,ecs_name as name,instance_id,ecs_status as status,county_city as region,cloud_category as cloud_platform,COALESCE(zone_name,'') as cloud_zone,ecs_type as instance_type,cpu_cores,memory_gb,system_disk as system_disk_type,system_disk_size_gb,'Linux' as os_type,ecs_os as os_name,ip_address,NULL as public_ip,customer_name,application_name,created_at::text,updated_at::text FROM scan_business_resources WHERE id=$1 AND deleted=0) t")
        .bind(id_num).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(
        serde_json::from_value(v).map_err(|e| AppError::internal(e.to_string()))?,
    )))
}
