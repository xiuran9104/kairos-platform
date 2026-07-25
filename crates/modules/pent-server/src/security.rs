use axum::{extract::Request, middleware::Next, response::Response};
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_security::{CurrentUser, DataScope, Permission};
use kairos_platform_framework_web::AppError;

use kairos_platform_pent_api::permission_codes;

pub fn tenant_id(user: &CurrentUser) -> Result<i64, AppError> {
    user.tenant_id
        .as_deref()
        .ok_or_else(|| AppError::forbidden("tenant context is required"))?
        .parse()
        .map_err(|_| AppError::forbidden("invalid tenant context"))
}

pub fn require(user: &CurrentUser, code: &str) -> Result<(), AppError> {
    if user.role_codes.iter().any(|role| role == "super_admin") {
        return Ok(());
    }
    let permission =
        Permission::new(code).map_err(|_| AppError::internal("invalid Pent permission policy"))?;
    if user.can(&permission) {
        Ok(())
    } else {
        Err(AppError::forbidden("permission denied"))
    }
}

pub fn tenant_wide(user: &CurrentUser) -> bool {
    user.role_codes.iter().any(|role| role == "super_admin")
        || matches!(user.data_scope, DataScope::All | DataScope::Organization)
}

pub async fn ensure_session_access(
    pool: &PgPool,
    session_id: &str,
    user: &CurrentUser,
) -> Result<(), AppError> {
    let tenant_id = tenant_id(user)?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pent_agent_sessions
             WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4)
         )",
    )
    .bind(session_id)
    .bind(tenant_id)
    .bind(tenant_wide(user))
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if allowed {
        Ok(())
    } else {
        Err(AppError::not_found("Pent session not found"))
    }
}

pub async fn ensure_run_access(
    pool: &PgPool,
    run_id: &str,
    user: &CurrentUser,
) -> Result<(), AppError> {
    let tenant_id = tenant_id(user)?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pent_runs
             WHERE id=$1 AND tenant_id=$2 AND ($3 OR user_id=$4)
         )",
    )
    .bind(run_id)
    .bind(tenant_id)
    .bind(tenant_wide(user))
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if allowed {
        Ok(())
    } else {
        Err(AppError::not_found("Pent run not found"))
    }
}

pub async fn ensure_turn_access(
    pool: &PgPool,
    turn_id: &str,
    user: &CurrentUser,
) -> Result<(), AppError> {
    let tenant_id = tenant_id(user)?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM pent_conversation_turns t
             JOIN pent_agent_sessions s ON s.id=t.session_id
             WHERE t.id=$1 AND s.tenant_id=$2 AND ($3 OR s.created_by=$4)
         )",
    )
    .bind(turn_id)
    .bind(tenant_id)
    .bind(tenant_wide(user))
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if allowed {
        Ok(())
    } else {
        Err(AppError::not_found("Pent turn not found"))
    }
}

pub async fn authorize_request(request: Request, next: Next) -> Result<Response, AppError> {
    let user = request
        .extensions()
        .get::<CurrentUser>()
        .ok_or_else(|| AppError::unauthorized("authentication required"))?;
    let path = request.uri().path();
    let method = request.method().as_str();
    let permission = required_permission(method, path);
    require(user, permission)?;
    tenant_id(user)?;
    Ok(next.run(request).await)
}

fn required_permission(method: &str, path: &str) -> &'static str {
    if path.starts_with("/pent/tools") {
        return if method == "GET" {
            permission_codes::TOOL_READ
        } else {
            permission_codes::TOOL_MANAGE
        };
    }
    if path.starts_with("/pent/approvals") {
        return if method == "GET" {
            permission_codes::APPROVAL_READ
        } else {
            permission_codes::APPROVAL_DECIDE
        };
    }
    if path.starts_with("/pent/artifacts") {
        return permission_codes::ARTIFACT_DOWNLOAD;
    }
    if path.starts_with("/pent/evidence") || path.contains("/evidence") {
        return permission_codes::EVIDENCE_READ;
    }
    if path.starts_with("/pent/findings") || path.contains("/findings") {
        return permission_codes::FINDING_READ;
    }
    if path.starts_with("/pent/sessions") && path.ends_with("/runs") {
        return permission_codes::RUN_EXECUTE;
    }
    if path.starts_with("/pent/sessions") || path.starts_with("/pent/turns") || path == "/pent/chat"
    {
        return permission_codes::AGENT_CHAT;
    }
    if path.starts_with("/pent/projects") {
        return if method == "GET" {
            permission_codes::PROJECT_READ
        } else {
            permission_codes::PROJECT_CREATE
        };
    }
    if path.starts_with("/pent/tasks") {
        return match method {
            "GET" => permission_codes::TASK_READ,
            "POST" if path.ends_with("/cancel") => permission_codes::TASK_CANCEL,
            "DELETE" => permission_codes::TASK_CANCEL,
            _ => permission_codes::TASK_CREATE,
        };
    }
    if path.starts_with("/pent/runs") {
        return if method == "GET" {
            permission_codes::RUN_READ
        } else {
            permission_codes::RUN_EXECUTE
        };
    }
    if path.starts_with("/pent/traffic") {
        return permission_codes::TRAFFIC_READ;
    }
    if path.starts_with("/pent/rmos") {
        return if method == "GET" {
            permission_codes::RMOS_READ
        } else {
            permission_codes::RMOS_FEEDBACK
        };
    }
    if path.starts_with("/pent/settings") {
        return if method == "GET" {
            permission_codes::SETTINGS_READ
        } else {
            permission_codes::SETTINGS_MANAGE
        };
    }
    permission_codes::DASHBOARD_READ
}

#[cfg(test)]
mod tests {
    use super::required_permission;
    use kairos_platform_pent_api::permission_codes;

    #[test]
    fn maps_mutating_routes_to_mutating_permissions() {
        assert_eq!(
            required_permission("POST", "/pent/runs"),
            permission_codes::RUN_EXECUTE
        );
        assert_eq!(
            required_permission("POST", "/pent/approvals/a/decision"),
            permission_codes::APPROVAL_DECIDE
        );
        assert_eq!(
            required_permission("PUT", "/pent/tools/nmap/versions/1"),
            permission_codes::TOOL_MANAGE
        );
    }
}
