//! DesRed-compatible Agent runtime configuration API.

use axum::{Json, Router, extract::State, routing::get};
use kairos_platform_framework_common::response::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{PentState, security};

pub fn routes() -> Router<PentState> {
    Router::new().route(
        "/pent/agent/settings",
        get(get_settings).put(update_settings),
    )
}

async fn get_settings(
    State(state): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    let config = sqlx::query_scalar::<_, Value>("SELECT agent_config || jsonb_build_object('inherited',tenant_id<>$1) FROM pent_runtime_policies WHERE tenant_id IN ($1,0) ORDER BY (tenant_id=$1) DESC LIMIT 1")
        .bind(tenant).fetch_optional(&state.pool).await.map_err(internal)?.unwrap_or_else(defaults);
    Ok(Json(ApiResponse::new(config)))
}

async fn update_settings(
    State(state): State<PentState>,
    user: CurrentUser,
    Json(config): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant = security::tenant_id(&user)?;
    validate(&config)?;
    let mut config = config;
    if let Some(values) = config.as_object_mut() {
        values.remove("inherited");
    }
    sqlx::query("INSERT INTO pent_runtime_policies(tenant_id,agent_config,updated_by) VALUES($1,$2,$3) ON CONFLICT(tenant_id) DO UPDATE SET agent_config=pent_runtime_policies.agent_config || EXCLUDED.agent_config,updated_by=EXCLUDED.updated_by,updated_at=now()")
        .bind(tenant).bind(config).bind(&user.user_id).execute(&state.pool).await.map_err(internal)?;
    Ok(Json(ApiResponse::new(
        json!({"message":"Agent 设置已保存"}),
    )))
}

fn validate(value: &Value) -> Result<(), AppError> {
    if !value.is_object() {
        return Err(AppError::bad_request("Agent 设置必须是对象"));
    }
    if !matches!(
        value["orchestration"].as_str().unwrap_or("deep"),
        "deep" | "plan_execute" | "supervisor"
    ) {
        return Err(AppError::bad_request("无效编排模式"));
    }
    for (key, min, max) in [
        ("max_iterations", 1, 10000),
        ("tool_search_min_tools", 1, 1000),
        ("tool_search_top_k", 1, 200),
        ("context_budget_tokens", 2000, 256000),
        ("run_retry_max_attempts", 0, 20),
        ("run_retry_max_backoff_seconds", 1, 300),
    ] {
        let number = value[key]
            .as_i64()
            .unwrap_or_else(|| defaults()[key].as_i64().unwrap_or(min));
        if !(min..=max).contains(&number) {
            return Err(AppError::bad_request(format!("{key} 超出允许范围")));
        }
    }
    Ok(())
}

fn defaults() -> Value {
    json!({"orchestration":"deep","max_iterations":3000,"sub_agent_user_context_max_chars":24000,"tool_search_enabled":true,"tool_search_min_tools":20,"tool_search_top_k":24,"context_budget_tokens":32000,"summarization_trigger_ratio":0.8,"run_retry_max_attempts":4,"run_retry_max_backoff_seconds":30,"max_tool_arguments_bytes":262144,"max_shell_command_bytes":65536,"inherited":true})
}
fn internal(error: sqlx::Error) -> AppError {
    AppError::internal(error.to_string())
}
