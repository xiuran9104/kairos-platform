use kairos_platform_framework_web::AppError;
use sqlx::PgPool;

pub const CLOUD_MANAGED: &str = "cloud_managed";
pub const DATACENTER_MANAGED: &str = "datacenter_managed";
pub const STANDALONE: &str = "standalone";
pub const PHYSICAL_STATUSES: [&str; 5] = [
    "available",
    "allocated",
    "maintenance",
    "offline",
    "retired",
];

pub fn validate_status(resource_type: &str, status: &str) -> Result<(), AppError> {
    if resource_type == "physical" && !PHYSICAL_STATUSES.contains(&status) {
        return Err(AppError::bad_request(
            "物理服务器状态仅支持空闲、已分配、维修中、已下线或已退役",
        ));
    }
    Ok(())
}

pub fn validate_placement(
    deployment_type: &str,
    cabinet_id: Option<i32>,
    start_u: Option<i32>,
    end_u: Option<i32>,
) -> Result<(), AppError> {
    if deployment_type == STANDALONE || deployment_type == CLOUD_MANAGED {
        return Ok(());
    }
    if cabinet_id.is_none() {
        return Err(AppError::bad_request("机房托管物理服务器必须选择机柜"));
    }
    let (start_u, end_u) = match (start_u, end_u) {
        (Some(start), Some(end)) => (start, end),
        _ => {
            return Err(AppError::bad_request(
                "机房托管物理服务器必须填写起始和结束 U 位",
            ));
        }
    };
    if !(1..=60).contains(&start_u) || !(1..=60).contains(&end_u) || end_u < start_u {
        return Err(AppError::bad_request(
            "U 位范围必须在 1-60 之间，且结束 U 位不能小于起始 U 位",
        ));
    }
    Ok(())
}

pub async fn validate_cabinet_placement(
    pool: &PgPool,
    deployment_type: &str,
    machine_room_id: Option<i32>,
    cabinet_id: Option<i32>,
    start_u: Option<i32>,
    end_u: Option<i32>,
    exclude_resource_id: Option<i32>,
) -> Result<(), AppError> {
    if deployment_type != DATACENTER_MANAGED {
        return Ok(());
    }
    let cabinet_id = cabinet_id.ok_or_else(|| AppError::bad_request("请选择机柜"))?;
    let room_id = machine_room_id.ok_or_else(|| AppError::bad_request("请选择物理机房"))?;
    let total_u:Option<i32>=sqlx::query_scalar("SELECT c.total_u FROM scan_cabinets c JOIN scan_cabinet_rows r ON r.id=c.row_id AND r.deleted=0 WHERE c.id=$1 AND r.machine_room_id=$2 AND c.deleted=0 AND c.status='active'")
        .bind(cabinet_id).bind(room_id).fetch_optional(pool).await.map_err(|e|AppError::internal(e.to_string()))?;
    let total_u = total_u.ok_or_else(|| AppError::bad_request("所选机柜不属于该机房或已停用"))?;
    let (start_u, end_u) = (start_u.unwrap_or(0), end_u.unwrap_or(0));
    if end_u > total_u {
        return Err(AppError::bad_request(format!(
            "结束 U 位超过机柜最大高度 {total_u}U"
        )));
    }
    let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM scan_business_resources WHERE cabinet_id=$1 AND deleted=0 AND resource_type='physical' AND ($4::bigint IS NULL OR id<>$4) AND rack_start_u<=$3 AND rack_end_u>=$2)")
        .bind(cabinet_id).bind(start_u).bind(end_u).bind(exclude_resource_id).fetch_one(pool).await.map_err(|e|AppError::internal(e.to_string()))?;
    if occupied {
        return Err(AppError::bad_request("所选 U 位已被其他服务器占用"));
    }
    Ok(())
}

pub async fn validate_cloud_binding(
    pool: &PgPool,
    deployment_type: &str,
    config_id: Option<i32>,
    room_id: Option<i32>,
) -> Result<(), AppError> {
    if deployment_type != CLOUD_MANAGED {
        return Ok(());
    }
    let config_id = config_id
        .ok_or_else(|| AppError::bad_request("云平台托管资源必须选择已认证的云平台对接配置"))?;
    let room_id = room_id
        .ok_or_else(|| AppError::bad_request("云平台托管资源必须选择对应平台下的物理机房"))?;
    let matches: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM scan_cloud_provider_configs c JOIN scan_machine_rooms r ON r.platform_id=c.platform_id WHERE c.id=$1 AND r.id=$2 AND c.deleted=0 AND r.deleted=0 AND c.status='active')")
        .bind(config_id).bind(room_id).fetch_one(pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    if !matches {
        return Err(AppError::bad_request(
            "对接配置未认证，或所选机房不属于该云平台",
        ));
    }
    Ok(())
}

pub fn validate(
    resource_type: &str,
    deployment_type: Option<&str>,
    machine_room_id: Option<i32>,
    management_ip: Option<&str>,
    business_ip: Option<&str>,
    network_cidr: Option<&str>,
    gateway: Option<&str>,
) -> Result<String, AppError> {
    if resource_type != "physical" {
        return Ok(CLOUD_MANAGED.to_string());
    }

    let deployment_type = deployment_type.unwrap_or(STANDALONE);
    if ![CLOUD_MANAGED, DATACENTER_MANAGED, STANDALONE].contains(&deployment_type) {
        return Err(AppError::bad_request("不支持的物理资源部署方式"));
    }
    if deployment_type != STANDALONE && machine_room_id.is_none() {
        return Err(AppError::bad_request(
            "云平台托管或机房托管的物理资源必须选择机房",
        ));
    }
    if deployment_type != CLOUD_MANAGED {
        let has_ip = management_ip.is_some_and(|v| !v.trim().is_empty())
            || business_ip.is_some_and(|v| !v.trim().is_empty());
        if !has_ip
            || network_cidr.is_none_or(|v| v.trim().is_empty())
            || gateway.is_none_or(|v| v.trim().is_empty())
        {
            return Err(AppError::bad_request(
                "机房托管或独立部署的物理资源必须填写管理/业务 IP、网段和网关",
            ));
        }
    }
    Ok(deployment_type.to_string())
}

#[cfg(test)]
mod tests {
    use super::validate_status;

    #[test]
    fn physical_status_is_a_closed_enum() {
        assert!(validate_status("physical", "available").is_ok());
        assert!(validate_status("physical", "allocated").is_ok());
        assert!(validate_status("physical", "随便填写").is_err());
        assert!(validate_status("cloud", "运行中").is_ok());
    }

    #[test]
    fn datacenter_placement_requires_valid_u_range() {
        assert!(
            super::validate_placement("datacenter_managed", Some(1), Some(10), Some(12)).is_ok()
        );
        assert!(
            super::validate_placement("datacenter_managed", Some(1), Some(12), Some(10)).is_err()
        );
        assert!(super::validate_placement("datacenter_managed", None, Some(10), Some(12)).is_err());
    }
}
