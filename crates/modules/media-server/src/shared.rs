use kairos_platform_framework_security::{CurrentUser, Permission};
use kairos_platform_framework_web::AppError;
use uuid::Uuid;

pub fn require(user: &CurrentUser, code: &str) -> Result<(), AppError> {
    let permission = Permission::new(code).map_err(|_| AppError::internal("invalid policy"))?;
    if user.can(&permission) {
        Ok(())
    } else {
        Err(AppError::forbidden("permission denied"))
    }
}

pub fn parse_id(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::bad_request("invalid id"))
}

pub fn current_user_id(user: &CurrentUser) -> Result<Uuid, AppError> {
    parse_id(&user.user_id)
}

pub fn affected(rows: u64, resource: &str) -> Result<(), AppError> {
    if rows == 0 {
        Err(AppError::not_found(format!("{resource} not found")))
    } else {
        Ok(())
    }
}
