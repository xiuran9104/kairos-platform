//! Tool management HTTP API (AI-02).
use crate::PentState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/tools", get(list_tools_v2).post(create_tool))
        .route(
            "/pent/tools/{tool_id}/versions/{version}",
            get(get_tool_version).put(update_tool_version),
        )
        .route(
            "/pent/tools/{tool_id}/versions/{version}/enable",
            post(enable_tool),
        )
        .route(
            "/pent/tools/{tool_id}/versions/{version}/disable",
            post(disable_tool),
        )
        .route(
            "/pent/tools/{tool_id}/versions/{version}/health-check",
            post(health_check),
        )
}

async fn list_tools_v2(
    State(s): State<PentState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
            'tool_id',tool_id,'version',version,'namespace',namespace,
            'name',name,'description',description,
            'risk_level',risk_level,'lifecycle_status',lifecycle_status,
            'capabilities',capabilities,'tags',tags,
            'revision',revision,'created_at',created_at
         ) FROM pent_tool_definitions
         WHERE lifecycle_status='active'
         ORDER BY namespace, name LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn get_tool_version(
    State(s): State<PentState>,
    Path((tool_id, version)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('tool_id',tool_id,'version',version,'namespace',namespace,'lifecycle_status',lifecycle_status,'definition',definition,'revision',revision) FROM pent_tool_definitions WHERE tool_id=$1 AND version=$2 ")
        .bind(&tool_id).bind(&version).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}

async fn create_tool(
    State(s): State<PentState>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tool_id = body["tool_id"].as_str().unwrap_or("");
    let version = body["version"].as_str().unwrap_or("1.0.0");
    let namespace = body["namespace"].as_str().unwrap_or("default");
    let definition = body.get("definition").cloned().unwrap_or_default();
    sqlx::query("INSERT INTO pent_tool_definitions(tool_id,version,namespace,status,definition,revision) VALUES($1,$2,$3,'active',$4,1) ON CONFLICT(tool_id,version) DO UPDATE SET definition=$4,revision=pent_tool_definitions.revision+1,updated_at=now()")
        .bind(tool_id).bind(version).bind(namespace).bind(&definition).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"工具注册成功","tool_id":tool_id,"version":version}),
    )))
}

async fn update_tool_version(
    State(s): State<PentState>,
    Path((tool_id, version)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let rev = sqlx::query_scalar::<_, i64>(
        "SELECT revision FROM pent_tool_definitions WHERE tool_id=$1 AND version=$2 ",
    )
    .bind(&tool_id)
    .bind(&version)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    let client_rev = body["revision"].as_i64().unwrap_or(0);
    if client_rev != 0 && client_rev != rev {
        return Err(AppError::bad_request(
            "revision conflict — optimistic lock failed",
        ));
    }
    let definition = body.get("definition").cloned().unwrap_or_default();
    sqlx::query("UPDATE pent_tool_definitions SET definition=$1,revision=revision+1,updated_at=now() WHERE tool_id=$2 AND version=$3 ")
        .bind(&definition).bind(&tool_id).bind(&version).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"工具更新成功"}))))
}

async fn enable_tool(
    State(s): State<PentState>,
    Path((tool_id, version)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE pent_tool_definitions SET lifecycle_status='active',updated_at=now() WHERE tool_id=$1 AND version=$2").bind(&tool_id).bind(&version).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"工具已启用"}))))
}

async fn disable_tool(
    State(s): State<PentState>,
    Path((tool_id, version)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE pent_tool_definitions SET lifecycle_status='disabled',updated_at=now() WHERE tool_id=$1 AND version=$2").bind(&tool_id).bind(&version).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"工具已禁用"}))))
}

async fn health_check(
    State(s): State<PentState>,
    Path((tool_id, version)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT tool_id,version FROM pent_tool_definitions WHERE tool_id=$1 AND version=$2",
    )
    .bind(&tool_id)
    .bind(&version)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    let prepared = crate::infrastructure::tool_registry::prepare_tool(
        &tool_id,
        &["health-check.invalid".to_string()],
        &crate::infrastructure::process::NetworkPolicy::Full,
    );
    let (healthy, detail) = match prepared {
        Ok(prepared) => {
            let image = prepared.container_image;
            let image_check = tokio::process::Command::new("/usr/bin/docker")
                .args([
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoDigests}}",
                    &image,
                ])
                .output()
                .await;
            let network = std::env::var("PENTA_DOCKER_TARGET_NETWORK").ok();
            let network_healthy = if let Some(network) = &network {
                tokio::process::Command::new("/usr/bin/docker")
                    .args(["network", "inspect", network])
                    .output()
                    .await
                    .is_ok_and(|output| output.status.success())
            } else {
                false
            };
            let image_digest_pinned = image.contains("@sha256:");
            let image_healthy = image_check
                .as_ref()
                .is_ok_and(|output| output.status.success());
            (
                image_healthy && network_healthy && image_digest_pinned,
                json!({
                    "backend": "docker",
                    "image": image,
                    "image_present": image_healthy,
                    "image_digest_pinned": image_digest_pinned,
                    "target_network": network,
                    "target_network_present": network_healthy
                }),
            )
        }
        Err(error) => (false, json!({"error": error})),
    };
    let status = if healthy { "healthy" } else { "unhealthy" };
    sqlx::query(
        "INSERT INTO pent_tool_health(tool_id,version,worker_id,status,detail,checked_at)
         VALUES($1,$2,'control-plane',$3,$4,now())
         ON CONFLICT(tool_id,version,worker_id) DO UPDATE SET
             status=excluded.status,detail=excluded.detail,checked_at=now()",
    )
    .bind(&row.0)
    .bind(&row.1)
    .bind(status)
    .bind(&detail)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"status":status,"detail":detail}),
    )))
}

#[derive(serde::Serialize)]
struct ApiResponse<T: serde::Serialize> {
    code: i32,
    data: T,
    message: String,
}
impl<T: serde::Serialize> ApiResponse<T> {
    fn new(data: T) -> Self {
        Self {
            code: 0,
            data,
            message: "ok".into(),
        }
    }
}
