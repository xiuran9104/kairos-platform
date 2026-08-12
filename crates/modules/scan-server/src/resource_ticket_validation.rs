use kairos_platform_framework_web::AppError;

pub fn validate_business_resource_type(resource_type: &str) -> Result<(), AppError> {
    match resource_type {
        "cloud" | "network" => Ok(()),
        "physical" => Err(AppError::bad_request(
            "物理设备请通过基础设施上架或分配工单办理",
        )),
        _ => Err(AppError::bad_request("不支持的业务资源类型")),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_business_resource_type;
    #[test]
    fn business_ticket_rejects_physical_devices() {
        assert!(validate_business_resource_type("cloud").is_ok());
        assert!(validate_business_resource_type("network").is_ok());
        assert!(validate_business_resource_type("physical").is_err());
    }
}
