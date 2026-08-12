use kairos_platform_framework_web::AppError;
use serde_json::Value;
use sqlx::PgPool;

pub async fn list(pool: &PgPool) -> Result<Vec<Value>, AppError> {
    sqlx::query_scalar::<_, Value>(r#"
        SELECT jsonb_build_object('id','scan-'||a.id,'record_id',a.id,'name',a.name,'ip',a.ip,'zone',a.zone,
            'ports','[]'::jsonb,'owner',a.owner,'weight',a.weight,'labels','[]'::jsonb,'os',a.os,
            'device_type',COALESCE(a.device_type,'Unknown'),'source_type','scan','source_label','扫描发现','editable',true,'status','active')
        FROM scan_assets a WHERE a.deleted=0
        UNION ALL
        SELECT jsonb_build_object('id','resource-'||b.id,'record_id',b.id,'name',b.ecs_name,
            'ip',COALESCE(NULLIF(b.management_ip,''),NULLIF(b.business_ip,''),b.ip_address,''),'zone','Intranet','ports','[]'::jsonb,
            'owner',b.customer_name,'weight',50,'labels','[]'::jsonb,'os',b.ecs_os,
            'device_type',CASE WHEN b.resource_type='cloud' THEN 'VM' ELSE 'BareMetal' END,
            'source_type',CASE WHEN b.resource_type='cloud' OR b.deployment_type='cloud_managed' THEN 'cloud_platform' ELSE 'physical_inventory' END,
            'source_label',CASE WHEN b.resource_type='cloud' THEN COALESCE(p.platform_name,'云平台同步') WHEN b.deployment_type='cloud_managed' THEN COALESCE(p.platform_name,'云平台托管') ELSE '基础设施台账' END,
            'editable',false,'status',b.ecs_status,'deployment_type',b.deployment_type)
        FROM scan_business_resources b
        LEFT JOIN scan_cloud_provider_configs c ON c.id=b.cloud_provider_config_id AND c.deleted=0
        LEFT JOIN scan_cloud_platforms p ON p.id=c.platform_id AND p.deleted=0
        WHERE b.deleted=0 AND b.resource_type IN ('cloud','physical')
        UNION ALL
        SELECT jsonb_build_object('id','device-'||d.id,'record_id',d.id,'name',d.name,
            'ip',COALESCE(d.management_ip,d.business_ip,''),'zone','Intranet','ports','[]'::jsonb,
            'owner',d.contact_person,'weight',50,'labels','[]'::jsonb,'os',NULL,
            'device_type',CASE WHEN d.device_type='network' THEN 'Network' ELSE 'Storage' END,
            'source_type','physical_inventory','source_label','基础设施台账','editable',false,'status',d.status)
        FROM scan_infrastructure_devices d WHERE d.deleted=0
        UNION ALL
        SELECT jsonb_build_object('id','security-'||s.id,'record_id',s.id,'name',s.name,
            'ip',COALESCE(s.management_ip,''),'zone','Intranet','ports','[]'::jsonb,
            'owner',s.contact_person,'weight',50,'labels','[]'::jsonb,'os',NULL,
            'device_type','Security','source_type','physical_inventory','source_label','基础设施台账','editable',false,'status',s.status)
        FROM scan_security_products s WHERE s.deleted=0
    "#).fetch_all(pool).await.map_err(|e| AppError::internal(e.to_string()))
}
