//! External MCP server management — CRUD over `pent_mcp_servers` plus
//! runtime start/stop against the shared `McpRegistry`.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use tracing::warn;

use crate::{ApiResponse, PentState, security};
use kairos_platform_pent_tooling::mcp::McpServerConfig;

/// PostgreSQL-backed deployment adapter for persisted MCP server configuration.
#[derive(Clone)]
pub struct PgMcpServerConfigSource {
    pool: kairos_platform_framework_database::PgPool,
}

impl PgMcpServerConfigSource {
    pub fn new(pool: kairos_platform_framework_database::PgPool) -> Self {
        Self { pool }
    }

    pub async fn load(&self) -> anyhow::Result<Vec<McpServerConfig>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                serde_json::Value,
                serde_json::Value,
                Option<String>,
                serde_json::Value,
                i32,
            ),
        >(
            "SELECT id, transport, command, args, env, url, headers, timeout_secs
             FROM pent_mcp_servers WHERE enabled",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, transport, command, args, env, url, headers, timeout_secs)| McpServerConfig {
                    id,
                    command: if transport == "stdio" {
                        command.unwrap_or_default()
                    } else {
                        String::new()
                    },
                    args: serde_json::from_value(args).unwrap_or_default(),
                    env: serde_json::from_value(env).unwrap_or_default(),
                    auto_start: true,
                    url: matches!(transport.as_str(), "http" | "sse")
                        .then_some(url)
                        .flatten(),
                    headers: serde_json::from_value(headers).unwrap_or_default(),
                    timeout_secs: u64::try_from(timeout_secs).unwrap_or(120).max(1),
                    max_concurrency: 4,
                },
            )
            .collect())
    }

    /// Apply the current database snapshot to a live registry.
    pub async fn load_into(
        &self,
        registry: &kairos_platform_pent_tooling::mcp::McpRegistry,
    ) -> anyhow::Result<usize> {
        let configs = self.load().await?;
        let mut connected = 0;
        for config in configs {
            let server_id = config.id.clone();
            match registry.add_server(config).await {
                Ok(()) => connected += 1,
                Err(error) => warn!(%error, %server_id, "failed to start configured MCP server"),
            }
        }
        Ok(connected)
    }
}

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/mcp/servers", get(list_servers).post(create_server))
        .route(
            "/pent/mcp/servers/{id}",
            get(get_server).put(update_server).delete(delete_server),
        )
        .route("/pent/mcp/servers/{id}/start", post(start_server))
        .route("/pent/mcp/servers/{id}/stop", post(stop_server))
}

/// Runtime status snapshot keyed by server id, empty when no registry.
async fn runtime_snapshot(s: &PentState) -> std::collections::HashMap<String, Value> {
    match &s.mcp_registry {
        Some(registry) => registry
            .snapshot()
            .await
            .into_iter()
            .filter_map(|entry| {
                let id = entry
                    .get("server_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                id.map(|id| (id, entry))
            })
            .collect(),
        None => Default::default(),
    }
}

/// Merge DB row with runtime status: connected/error from the registry,
/// `disconnected` for enabled servers not in the registry, `disabled` otherwise.
fn with_runtime_status(
    mut server: Value,
    snapshot: &std::collections::HashMap<String, Value>,
) -> Value {
    let enabled = server
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let id = server
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let runtime = snapshot.get(&id).cloned();
    let Some(object) = server.as_object_mut() else {
        return server;
    };
    if !enabled {
        object.insert("status".into(), json!("disabled"));
        object.insert("tool_count".into(), json!(0));
        object.insert("error".into(), Value::Null);
    } else if let Some(runtime) = runtime {
        object.insert(
            "status".into(),
            runtime.get("status").cloned().unwrap_or(json!("error")),
        );
        object.insert(
            "tool_count".into(),
            runtime.get("tool_count").cloned().unwrap_or(json!(0)),
        );
        object.insert(
            "error".into(),
            runtime.get("error").cloned().unwrap_or(Value::Null),
        );
    } else {
        object.insert("status".into(), json!("disconnected"));
        object.insert("tool_count".into(), json!(0));
        object.insert("error".into(), Value::Null);
    }
    server
}

fn validate_config(body: &Value) -> Result<(&str, &str), AppError> {
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("name 不能为空"))?;
    let transport = body
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    if !matches!(transport, "stdio" | "http" | "sse") {
        return Err(AppError::bad_request("transport 必须是 stdio/http/sse"));
    }
    let command = body.get("command").and_then(Value::as_str).unwrap_or("");
    let url = body.get("url").and_then(Value::as_str).unwrap_or("");
    if transport == "stdio" && command.trim().is_empty() {
        return Err(AppError::bad_request("stdio 传输必须提供 command"));
    }
    if matches!(transport, "http" | "sse") && url.trim().is_empty() {
        return Err(AppError::bad_request("http/sse 传输必须提供 url"));
    }
    Ok((name, transport))
}

async fn list_servers(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_mcp_servers t
         WHERE t.tenant_id=$1 ORDER BY t.created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let snapshot = runtime_snapshot(&s).await;
    let servers: Vec<Value> = rows
        .into_iter()
        .map(|row| with_runtime_status(row, &snapshot))
        .collect();
    let enabled = servers
        .iter()
        .filter(|server| server["enabled"].as_bool() == Some(true))
        .count();
    let connected = servers
        .iter()
        .filter(|server| server["status"].as_str() == Some("connected"))
        .count();
    Ok(Json(ApiResponse::new(json!({
        "servers": servers,
        "stats": {
            "total": servers.len(),
            "enabled": enabled,
            "disabled": servers.len() - enabled,
            "connected": connected,
        },
    }))))
}

async fn create_server(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let (name, transport) = validate_config(&body)?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pent_mcp_servers(
             id, tenant_id, name, transport, command, args, env, url,
             headers, enabled, timeout_secs, description
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(name)
    .bind(transport)
    .bind(body.get("command").and_then(Value::as_str))
    .bind(body.get("args").cloned().unwrap_or_else(|| json!([])))
    .bind(body.get("env").cloned().unwrap_or_else(|| json!({})))
    .bind(body.get("url").and_then(Value::as_str))
    .bind(body.get("headers").cloned().unwrap_or_else(|| json!({})))
    .bind(body.get("enabled").and_then(Value::as_bool).unwrap_or(true))
    .bind(
        body.get("timeout_secs")
            .and_then(Value::as_i64)
            .unwrap_or(60) as i32,
    )
    .bind(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(format!("创建 MCP 服务器失败（名称重复？）：{error}")))?;
    get_server(State(s), Path(id), user).await
}

async fn get_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_mcp_servers t WHERE t.id=$1 AND t.tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("MCP 服务器不存在"))?;
    let snapshot = runtime_snapshot(&s).await;
    Ok(Json(ApiResponse::new(with_runtime_status(row, &snapshot))))
}

async fn update_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    if let Some(transport) = body.get("transport").and_then(Value::as_str) {
        if !matches!(transport, "stdio" | "http" | "sse") {
            return Err(AppError::bad_request("transport 必须是 stdio/http/sse"));
        }
    }
    let affected = sqlx::query(
        "UPDATE pent_mcp_servers SET
             name=COALESCE($3, name),
             transport=COALESCE($4, transport),
             command=COALESCE($5, command),
             args=COALESCE($6, args),
             env=COALESCE($7, env),
             url=COALESCE($8, url),
             headers=COALESCE($9, headers),
             enabled=COALESCE($10, enabled),
             timeout_secs=COALESCE($11, timeout_secs),
             description=COALESCE($12, description),
             updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("transport").and_then(Value::as_str))
    .bind(body.get("command").and_then(Value::as_str))
    .bind(body.get("args").cloned())
    .bind(body.get("env").cloned())
    .bind(body.get("url").and_then(Value::as_str))
    .bind(body.get("headers").cloned())
    .bind(body.get("enabled").and_then(Value::as_bool))
    .bind(
        body.get("timeout_secs")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
    )
    .bind(body.get("description").and_then(Value::as_str))
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("MCP 服务器不存在"));
    }
    get_server(State(s), Path(id), user).await
}

async fn delete_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query("DELETE FROM pent_mcp_servers WHERE id=$1 AND tenant_id=$2")
        .bind(&id)
        .bind(tenant_id)
        .execute(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("MCP 服务器不存在"));
    }
    if let Some(registry) = &s.mcp_registry {
        registry.remove_server(&id).await;
    }
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn start_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_mcp_servers t WHERE t.id=$1 AND t.tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("MCP 服务器不存在"))?;
    let transport = row["transport"].as_str().unwrap_or("stdio");
    let registry = s
        .mcp_registry
        .as_ref()
        .ok_or_else(|| AppError::bad_request("MCP 运行时未启用"))?;
    let config = match transport {
        "http" | "sse" => {
            let url = row["url"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::bad_request("http/sse 传输缺少 url"))?;
            McpServerConfig {
                id: id.clone(),
                command: String::new(),
                args: vec![],
                env: Default::default(),
                auto_start: true,
                url: Some(url.to_string()),
                headers: serde_json::from_value(row["headers"].clone()).unwrap_or_default(),
                timeout_secs: row["timeout_secs"].as_u64().unwrap_or(60).max(1),
                max_concurrency: 4,
            }
        }
        _ => {
            let command = row["command"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::bad_request("stdio 传输缺少 command"))?;
            McpServerConfig {
                id: id.clone(),
                command: command.to_string(),
                args: serde_json::from_value(row["args"].clone()).unwrap_or_default(),
                env: serde_json::from_value(row["env"].clone()).unwrap_or_default(),
                auto_start: true,
                url: None,
                headers: Default::default(),
                timeout_secs: row["timeout_secs"].as_u64().unwrap_or(60).max(1),
                max_concurrency: 4,
            }
        }
    };
    registry
        .add_server(config)
        .await
        .map_err(|error| AppError::internal(format!("MCP 服务器连接失败：{error}")))?;
    Ok(Json(ApiResponse::new(json!({"status": "connected"}))))
}

async fn stop_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pent_mcp_servers WHERE id=$1 AND tenant_id=$2)",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if !exists {
        return Err(AppError::not_found("MCP 服务器不存在"));
    }
    if let Some(registry) = &s.mcp_registry {
        registry.remove_server(&id).await;
    }
    Ok(Json(ApiResponse::new(json!({"status": "disabled"}))))
}
