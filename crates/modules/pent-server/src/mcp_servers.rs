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
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
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

type ReconcileRow = (
    String,
    String,
    Option<String>,
    Value,
    Value,
    Option<String>,
    Value,
    i32,
    bool,
    i64,
    Option<i64>,
);

fn config_from_row(row: &ReconcileRow) -> McpServerConfig {
    let (id, transport, command, args, env, url, headers, timeout_secs, ..) = row;
    McpServerConfig {
        id: id.clone(),
        command: (transport == "stdio")
            .then(|| command.clone().unwrap_or_default())
            .unwrap_or_default(),
        args: serde_json::from_value(args.clone()).unwrap_or_default(),
        env: serde_json::from_value(env.clone()).unwrap_or_default(),
        auto_start: true,
        url: matches!(transport.as_str(), "http" | "sse")
            .then(|| url.clone())
            .flatten(),
        headers: serde_json::from_value(headers.clone()).unwrap_or_default(),
        timeout_secs: u64::try_from(*timeout_secs).unwrap_or(120).max(1),
        max_concurrency: 4,
    }
}

async fn record_instance(
    pool: &kairos_platform_framework_database::PgPool,
    server_id: &str,
    worker_id: &str,
    status: &str,
    revision: i64,
    tool_count: i32,
    error: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO pent_mcp_server_instances(
             server_id,worker_id,status,applied_revision,tool_count,error,last_heartbeat
         ) VALUES($1,$2,$3,$4,$5,$6,now())
         ON CONFLICT(server_id,worker_id) DO UPDATE SET
             status=EXCLUDED.status,applied_revision=EXCLUDED.applied_revision,
             tool_count=EXCLUDED.tool_count,error=EXCLUDED.error,last_heartbeat=now()",
    )
    .bind(server_id)
    .bind(worker_id)
    .bind(status)
    .bind(revision)
    .bind(tool_count)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

async fn reconcile_mcp_servers(
    pool: &kairos_platform_framework_database::PgPool,
    registry: &kairos_platform_pent_tooling::mcp::McpRegistry,
    worker_id: &str,
    managed: &mut HashSet<String>,
) -> anyhow::Result<()> {
    let rows = sqlx::query_as::<_, ReconcileRow>(
        "SELECT s.id,s.transport,s.command,s.args,s.env,s.url,s.headers,s.timeout_secs,
                s.enabled,s.config_revision,i.applied_revision
         FROM pent_mcp_servers s
         LEFT JOIN pent_mcp_server_instances i
           ON i.server_id=s.id AND i.worker_id=$1",
    )
    .bind(worker_id)
    .fetch_all(pool)
    .await?;
    let configured: HashSet<String> = rows.iter().map(|row| row.0.clone()).collect();
    for server_id in managed.difference(&configured) {
        registry.remove_server(server_id).await;
    }
    *managed = configured;
    for row in rows {
        let server_id = row.0.clone();
        let enabled = row.8;
        let revision = row.9;
        let applied_revision = row.10.unwrap_or(0);
        if !enabled {
            registry.remove_server(&server_id).await;
            record_instance(pool, &server_id, worker_id, "stopped", revision, 0, None).await?;
            continue;
        }
        if applied_revision == revision {
            let healthy = registry.is_healthy(&server_id).await;
            let (status, tool_count, error) = if healthy {
                (
                    "connected",
                    registry
                        .tools(&server_id)
                        .await
                        .map(|v| v.len() as i32)
                        .unwrap_or(0),
                    None,
                )
            } else {
                ("error", 0, Some("MCP connection is not active"))
            };
            record_instance(
                pool, &server_id, worker_id, status, revision, tool_count, error,
            )
            .await?;
            continue;
        }
        registry.remove_server(&server_id).await;
        record_instance(pool, &server_id, worker_id, "connecting", revision, 0, None).await?;
        match registry.add_server(config_from_row(&row)).await {
            Ok(()) => {
                let count = registry
                    .tools(&server_id)
                    .await
                    .map(|v| v.len() as i32)
                    .unwrap_or(0);
                record_instance(
                    pool,
                    &server_id,
                    worker_id,
                    "connected",
                    revision,
                    count,
                    None,
                )
                .await?;
            }
            Err(error) => {
                warn!(%error, %server_id, %worker_id, "MCP reconciliation failed");
                record_instance(
                    pool,
                    &server_id,
                    worker_id,
                    "error",
                    revision,
                    0,
                    Some(&error.to_string()),
                )
                .await?;
            }
        }
    }
    Ok(())
}

/// Start durable MCP desired-state reconciliation for one Pent Agent replica.
pub fn spawn_mcp_reconciler(
    pool: kairos_platform_framework_database::PgPool,
    registry: Arc<kairos_platform_pent_tooling::mcp::McpRegistry>,
    worker_id: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut managed = HashSet::new();
        loop {
            if let Err(error) =
                reconcile_mcp_servers(&pool, &registry, &worker_id, &mut managed).await
            {
                warn!(%error, %worker_id, "MCP reconciliation cycle failed");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
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

async fn runtime_snapshot(s: &PentState, tenant_id: i64) -> HashMap<String, Vec<Value>> {
    let rows = sqlx::query_as::<_, (String, Value)>(
        "SELECT i.server_id,jsonb_build_object(
             'worker_id',i.worker_id,
             'status',CASE WHEN i.last_heartbeat < now()-interval '30 seconds' THEN 'stale' ELSE i.status END,
             'applied_revision',i.applied_revision,
             'tool_count',i.tool_count,
             'error',i.error,
             'last_heartbeat',i.last_heartbeat
         )
         FROM pent_mcp_server_instances i
         JOIN pent_mcp_servers s ON s.id=i.server_id
         WHERE s.tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_all(&s.pool)
    .await
    .unwrap_or_default();
    let mut snapshot: HashMap<String, Vec<Value>> = HashMap::new();
    for (server_id, instance) in rows {
        snapshot.entry(server_id).or_default().push(instance);
    }
    snapshot
}

/// Merge DB row with runtime status: connected/error from the registry,
/// `disconnected` for enabled servers not in the registry, `disabled` otherwise.
fn with_runtime_status(mut server: Value, snapshot: &HashMap<String, Vec<Value>>) -> Value {
    let enabled = server
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let id = server
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let instances = snapshot.get(&id).cloned().unwrap_or_default();
    let Some(object) = server.as_object_mut() else {
        return server;
    };
    let revision = object
        .get("config_revision")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    if !enabled {
        let stopped = instances.iter().all(|instance| {
            instance.get("applied_revision").and_then(Value::as_i64) == Some(revision)
                && matches!(instance.get("status").and_then(Value::as_str), Some("stopped" | "stale"))
        });
        object.insert(
            "status".into(),
            json!(if stopped { "disabled" } else { "stopping" }),
        );
        object.insert("tool_count".into(), json!(0));
        object.insert("error".into(), Value::Null);
    } else {
        let current: Vec<&Value> = instances
            .iter()
            .filter(|instance| {
                instance.get("applied_revision").and_then(Value::as_i64) == Some(revision)
                    && instance.get("status").and_then(Value::as_str) != Some("stale")
            })
            .collect();
        let status = if current.iter().any(|v| v["status"] == "connected") {
            "connected"
        } else if current.iter().any(|v| v["status"] == "connecting") {
            "connecting"
        } else if current.iter().any(|v| v["status"] == "error") {
            "error"
        } else {
            "pending"
        };
        let tool_count: i64 = current
            .iter()
            .filter_map(|v| v["tool_count"].as_i64())
            .sum();
        let error = current
            .iter()
            .find_map(|v| v["error"].as_str())
            .map(str::to_owned);
        object.insert("status".into(), json!(status));
        object.insert("tool_count".into(), json!(tool_count));
        object.insert(
            "error".into(),
            error.map(Value::String).unwrap_or(Value::Null),
        );
    }
    object.insert("instances".into(), Value::Array(instances));
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
    let snapshot = runtime_snapshot(&s, tenant_id).await;
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
    let snapshot = runtime_snapshot(&s, tenant_id).await;
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
             config_revision=config_revision+1,
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
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn start_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let updated = sqlx::query_scalar::<_, i64>(
        "UPDATE pent_mcp_servers
         SET enabled=true,config_revision=config_revision+1,updated_at=now()
         WHERE id=$1 AND tenant_id=$2 RETURNING config_revision",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("MCP 服务器不存在"))?;
    Ok(Json(ApiResponse::new(
        json!({"status": "pending", "config_revision": updated}),
    )))
}

async fn stop_server(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let updated = sqlx::query_scalar::<_, i64>(
        "UPDATE pent_mcp_servers
         SET enabled=false,config_revision=config_revision+1,updated_at=now()
         WHERE id=$1 AND tenant_id=$2 RETURNING config_revision",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("MCP 服务器不存在"))?;
    Ok(Json(ApiResponse::new(
        json!({"status": "stopping", "config_revision": updated}),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_status_uses_current_live_agent_revision() {
        let server = json!({"id":"mcp-1","enabled":true,"config_revision":3});
        let snapshot = HashMap::from([(
            "mcp-1".to_string(),
            vec![
                json!({"status":"connected","applied_revision":2,"tool_count":9}),
                json!({"status":"connected","applied_revision":3,"tool_count":4}),
                json!({"status":"stale","applied_revision":3,"tool_count":7}),
            ],
        )]);
        let projected = with_runtime_status(server, &snapshot);
        assert_eq!(projected["status"], "connected");
        assert_eq!(projected["tool_count"], 4);
        assert_eq!(projected["instances"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn disabled_server_stays_stopping_until_agents_apply_revision() {
        let server = json!({"id":"mcp-1","enabled":false,"config_revision":4});
        let snapshot = HashMap::from([(
            "mcp-1".to_string(),
            vec![json!({"status":"connected","applied_revision":3,"tool_count":2})],
        )]);
        assert_eq!(with_runtime_status(server, &snapshot)["status"], "stopping");
    }
}
