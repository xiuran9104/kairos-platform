//! Agent role management — a role is a prompt preset (`user_prompt`) plus an
//! optional tool whitelist and HITL policy. Roles are tenant-scoped; tenant 0
//! rows are platform presets visible to every tenant. Binding a role to a
//! session changes the model's prompt and tool surface (see `context_source`
//! and `model_gateway`).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{ApiResponse, PentState, security};

const MAX_PROMPT_CHARS: usize = 12_000;
const MAX_TOOLS: usize = 200;

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/roles", get(list_roles).post(create_role))
        .route("/pent/roles/{id}", get(get_role).put(update_role).delete(delete_role))
        .route("/pent/roles/{id}/status", put(update_role_status))
}

async fn list_roles(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let configured_default = crate::role_resolution::configured_default_role_id(&s.pool, tenant_id)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .unwrap_or_else(|| crate::role_resolution::PLATFORM_DEFAULT_ROLE_ID.to_string());
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(r) || jsonb_build_object(
             'inherited', r.tenant_id = 0 AND r.tenant_id <> $1,
             'is_configured_default', r.id = $2,
             'explicit_session_count', (
                 SELECT count(*) FROM pent_agent_sessions s
                 WHERE s.tenant_id=$1 AND s.constraints->>'role_id'=r.id
             ),
             'project_count', (
                 SELECT count(*) FROM pent_projects p
                 WHERE p.tenant_id=$1 AND p.default_role_id=r.id AND p.deleted=0
             ),
             'historical_run_count', (
                 SELECT count(*) FROM pent_runs pr
                 WHERE pr.tenant_id=$1
                   AND pr.policy_snapshot #>> '{role,resolved_role_id}'=r.id
             )
         )
         FROM pent_agent_roles r
         WHERE (r.tenant_id = $1 OR r.tenant_id = 0)
         ORDER BY (r.id=$2) DESC, r.is_default DESC, r.tenant_id ASC, r.name",
    )
    .bind(tenant_id)
    .bind(configured_default)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn get_role(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let value = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(r) || jsonb_build_object('inherited', r.tenant_id = 0 AND r.tenant_id <> $2)
         FROM pent_agent_roles r
         WHERE r.id=$1 AND (r.tenant_id = $2 OR r.tenant_id = 0)",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("角色不存在"))?;
    Ok(Json(ApiResponse::new(value)))
}

fn validate_payload(body: &Value) -> Result<(String, String, String, String, Value, Value), AppError> {
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("角色名称不能为空"))?
        .to_string();
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let icon = body
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(20)
        .collect();
    let user_prompt = body
        .get("user_prompt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if user_prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(AppError::bad_request(format!(
            "提示词预设不能超过 {MAX_PROMPT_CHARS} 字符"
        )));
    }
    let tool_whitelist = body.get("tool_whitelist").cloned().unwrap_or_else(|| json!([]));
    if !tool_whitelist.is_array() {
        return Err(AppError::bad_request("tool_whitelist 必须是数组"));
    }
    let tools: Vec<&str> = tool_whitelist
        .as_array()
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if tools.len() > MAX_TOOLS {
        return Err(AppError::bad_request(format!("工具白名单不能超过 {MAX_TOOLS} 项")));
    }
    let hitl_policy = body.get("hitl_policy").cloned().unwrap_or_else(|| json!({}));
    if !hitl_policy.is_object() {
        return Err(AppError::bad_request("hitl_policy 必须是对象"));
    }
    Ok((name, description, icon, user_prompt, tool_whitelist, hitl_policy))
}

async fn create_role(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let (name, description, icon, user_prompt, tool_whitelist, hitl_policy) =
        validate_payload(&body)?;
    let id = Uuid::new_v4().to_string();
    let row = sqlx::query_scalar::<_, Value>(
        "INSERT INTO pent_agent_roles(
             id, tenant_id, name, description, icon, user_prompt,
             tool_whitelist, hitl_policy, enabled, is_default
         ) VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, false)
         RETURNING to_jsonb(pent_agent_roles.*)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(&name)
    .bind(&description)
    .bind(&icon)
    .bind(&user_prompt)
    .bind(&tool_whitelist)
    .bind(&hitl_policy)
    .bind(body.get("enabled").and_then(Value::as_bool).unwrap_or(true))
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(row)))
}

async fn update_role(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let (name, description, icon, user_prompt, tool_whitelist, hitl_policy) =
        validate_payload(&body)?;
    let affected = sqlx::query(
        "UPDATE pent_agent_roles
         SET name=$3, description=$4, icon=$5, user_prompt=$6,
             tool_whitelist=$7, hitl_policy=$8,
             enabled=COALESCE($9, enabled), updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(&name)
    .bind(&description)
    .bind(&icon)
    .bind(&user_prompt)
    .bind(&tool_whitelist)
    .bind(&hitl_policy)
    .bind(body.get("enabled").and_then(Value::as_bool))
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("角色不存在或为平台预设，无法修改"));
    }
    get_role(State(s), Path(id), user).await
}

async fn update_role_status(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let enabled = body
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::bad_request("enabled 必须是布尔值"))?;
    let affected = sqlx::query(
        "UPDATE pent_agent_roles SET enabled=$3, updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(enabled)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("角色不存在或为平台预设，无法修改"));
    }
    get_role(State(s), Path(id), user).await
}

async fn delete_role(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let mut tx = s.pool.begin().await.map_err(|error| AppError::internal(error.to_string()))?;
    let deletable: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pent_agent_roles
             WHERE id=$1 AND tenant_id=$2 AND is_default=false
         )",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if !deletable {
        return Err(AppError::bad_request("角色不存在、为平台预设或默认角色，无法删除"));
    }
    sqlx::query(
        "UPDATE pent_agent_sessions
         SET constraints=constraints-'role_id',updated_at=now()
         WHERE tenant_id=$2 AND constraints->>'role_id'=$1",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE pent_projects SET default_role_id=NULL,updated_at=now()
         WHERE tenant_id=$2 AND default_role_id=$1 AND deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE pent_runtime_policies SET
             agent_config=jsonb_set(
                 agent_config,
                 '{platform_settings,robots,default_role_id}',
                 to_jsonb($3::text),
                 true
             ),
             updated_at=now()
         WHERE tenant_id=$2
           AND agent_config #>> '{platform_settings,robots,default_role_id}'=$1",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(crate::role_resolution::PLATFORM_DEFAULT_ROLE_ID)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let affected = sqlx::query(
        "DELETE FROM pent_agent_roles
         WHERE id=$1 AND tenant_id=$2 AND is_default=false",
    )
    .bind(&id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("角色删除失败，请刷新后重试"));
    }
    tx.commit().await.map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"deleted": id}))))
}
