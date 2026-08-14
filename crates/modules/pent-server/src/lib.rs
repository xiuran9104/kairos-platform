pub mod application;
pub mod cognition;
pub mod config;
pub mod conversation;
pub mod infrastructure;
pub(crate) mod report;
mod agents;
mod assets;
mod facts;
mod files;
mod infocollect;
mod knowledge_logs;
mod mcp_servers;
mod monitor;
mod runtime_policy;
mod roles;
mod terminal;
mod security;
mod session;
mod skills;
pub mod tooling;
pub mod transport;
mod workflows;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    middleware::from_fn,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use uuid::Uuid;

pub use config::PentConfig;

#[derive(Clone)]
pub struct PentState {
    pub pool: PgPool,
    conversation_tasks: conversation::turn_tasks::TurnTaskRegistry,
    pub nats: Option<crate::transport::mq::NatsHandle>,
    pub mcp_registry: Option<std::sync::Arc<crate::tooling::mcp::registry::McpRegistry>>,
}

impl PentState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            conversation_tasks: Default::default(),
            nats: None,
            mcp_registry: None,
        }
    }

    pub fn with_nats(mut self, nats: crate::transport::mq::NatsHandle) -> Self {
        self.nats = Some(nats);
        self
    }

    pub fn with_mcp(mut self, mcp: std::sync::Arc<crate::tooling::mcp::registry::McpRegistry>) -> Self {
        self.mcp_registry = Some(mcp);
        self
    }
}

pub fn routes(state: PentState) -> Router {
    let protected = Router::new()
        .route("/pent/projects", get(list_projects).post(create_project))
        .route(
            "/pent/projects/{id}",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/pent/projects/{id}/model", put(update_project_model))
        .route(
            "/pent/settings/model-bindings",
            get(get_model_bindings).put(update_model_bindings),
        )
        .route(
            "/pent/settings/sections/{section}",
            get(get_platform_section).put(update_platform_section),
        )
        .route("/pent/settings/robots", get(get_robot_settings).put(update_robot_settings))
        .route("/pent/tasks", get(list_tasks).post(create_task))
        .route("/pent/tasks/{id}", get(get_task).delete(delete_task))
        .route("/pent/tasks/{id}/cancel", post(cancel_task))
        .route("/pent/runs", get(list_runs).post(create_run))
        .route("/pent/runs/batch", post(create_batch_runs))
        .route("/pent/runs/{id}", get(get_run).delete(delete_run))
        .route("/pent/runs/{id}/cancel", post(cancel_run))
        .route("/pent/runs/{id}/pause", post(pause_run))
        .route("/pent/runs/{id}/resume", post(resume_run))
        .route("/pent/runs/{id}/events", get(get_run_events))
        .route("/pent/runs/{id}/tool-calls", get(get_tool_calls))
        .route("/pent/runs/{id}/plan", get(get_run_plan))
        // /pent/chat removed — was a demo stub with keyword-fixed replies
        .merge(tooling::http::routes())
        .route("/pent/nodes", get(list_nodes))
        .route("/pent/metrics", get(metrics))
        .route("/pent/tools/overview", get(tools_overview))
        .route("/pent/traffic", get(traffic_list))
        .route("/pent/rmos/memories", get(rmos_memories))
        .route("/pent/rmos/memories/{id}/feedback", post(rmos_feedback))
        .route("/pent/rmos/global-search", get(rmos_global_search))
        .route("/pent/rmos/attack-graph", get(attack_graph))
        .route(
            "/pent/settings/runtime-policy",
            get(get_runtime_policy).put(update_runtime_policy),
        )
        .route("/pent/runs/{id}/events/stream", get(run_events_sse))
        .route("/pent/runs/{id}/evidence", get(get_run_evidence))
        .route("/pent/runs/{id}/findings", get(get_run_findings))
        .route("/pent/runs/{id}/artifacts", get(get_run_artifacts))
        .route("/pent/runs/{id}/traffic", get(get_run_traffic))
        .route("/pent/evidence/{id}", get(get_evidence))
        .route("/pent/findings/{id}", get(get_finding).post(review_finding))
        .route("/pent/projects/{id}/findings", get(get_project_findings))
        .route("/api/projects/{id}/vulnerabilities", get(get_project_findings))
        .route("/pent/projects/{id}/traffic", get(get_project_traffic))
        .route("/pent/traffic/{id}", get(get_traffic))
        .route("/pent/artifacts/{id}/download", get(download_artifact))
        .merge(session::routes())
        .merge(conversation::http::routes())
        .merge(conversation::agent_http::routes())
        .merge(conversation::hitl_http::routes())
        .merge(skills::routes())
        .merge(roles::routes())
        .merge(monitor::routes())
        .merge(infocollect::routes())
        .merge(mcp_servers::routes())
        .merge(knowledge_logs::routes())
        .merge(agents::routes())
        .merge(workflows::routes())
        .merge(files::routes())
        .merge(terminal::routes())
        .merge(assets::routes())
        .merge(facts::routes())
        .route_layer(from_fn(security::authorize_request));

    Router::new()
        .route("/pent/health", get(health))
        .merge(protected)
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

fn normalize_confirmed_scope(scope: Value, confirmed_by: &str) -> Result<Value, AppError> {
    let targets = match scope {
        Value::Array(targets) => targets,
        Value::Object(mut object) => object
            .remove("targets")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let targets: Vec<Value> = targets
        .into_iter()
        .filter_map(|target| target.as_str().map(str::trim).map(str::to_owned))
        .filter(|target| !target.is_empty())
        .map(Value::String)
        .collect();
    if targets.is_empty() {
        return Err(AppError::bad_request("至少需要一个明确授权的目标"));
    }
    Ok(json!({
        "targets": targets,
        "confirmed": true,
        "confirmed_at": chrono::Utc::now(),
        "confirmed_by": confirmed_by
    }))
}

async fn ensure_project_access(
    pool: &PgPool,
    project_id: &str,
    user: &CurrentUser,
) -> Result<(), AppError> {
    let tenant_id = security::tenant_id(user)?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pent_projects
             WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0
         )",
    )
    .bind(project_id)
    .bind(tenant_id)
    .bind(security::tenant_wide(user))
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if exists {
        Ok(())
    } else {
        Err(AppError::bad_request("项目不存在或无权访问"))
    }
}

async fn list_projects(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) || jsonb_build_object(
             'model_config_id', (
                 SELECT model_config_id FROM pent_model_role_bindings
                 WHERE project_id=t.id AND model_role='conversation' AND enabled=true LIMIT 1
             )
         )
         FROM pent_projects t
         WHERE t.tenant_id=$1 AND ($2 OR t.created_by=$3) AND t.deleted=0
         ORDER BY t.created_at DESC LIMIT 100")
        .bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
        .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_project(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) || jsonb_build_object(
             'model_config_id', (
                 SELECT model_config_id FROM pent_model_role_bindings
                 WHERE project_id=t.id AND model_role='conversation' AND enabled=true
                 LIMIT 1
             )
         )
         FROM pent_projects t
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn create_project(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let id = Uuid::new_v4().to_string();
    let name = body["name"].as_str().unwrap_or("untitled");
    let desc = body.get("description").and_then(|v| v.as_str());
    let scope = body.get("asset_scope").cloned().unwrap_or_default();
    sqlx::query("INSERT INTO pent_projects(id,tenant_id,name,description,asset_scope,created_by) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(&id).bind(tenant_id).bind(name).bind(desc).bind(&scope).bind(&user.user_id)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"项目创建成功"}),
    )))
}

async fn update_project_model(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let model_config_id = body["model_config_id"].as_i64().filter(|&v| v > 0)
        .ok_or_else(|| AppError::bad_request("请选择有效的模型"))?;
    sqlx::query(
        "INSERT INTO pent_model_role_bindings(tenant_id,project_id,model_role,model_config_id,temperature,max_output_tokens,timeout_seconds,enabled,revision,updated_at)
         VALUES($1,$2,'conversation',$3,0.7,4096,120,true,1,now())
         ON CONFLICT(tenant_id,project_id,model_role) DO UPDATE SET
         model_config_id=EXCLUDED.model_config_id,revision=pent_model_role_bindings.revision+1,updated_at=now()",
    )
    .bind(tenant_id).bind(&id).bind(model_config_id)
    .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"项目模型已更新"}))))
}

async fn update_project(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let name = body["name"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("项目名称不能为空"))?;
    let description = body.get("description").and_then(Value::as_str);
    let asset_scope = body
        .get("asset_scope")
        .cloned()
        .unwrap_or_else(|| json!([]));
    if !asset_scope.is_array() {
        return Err(AppError::bad_request("资产范围必须是数组"));
    }
    let status = body
        .get("status")
        .and_then(Value::as_str);
    if let Some(status) = status {
        if !matches!(status, "active" | "paused" | "archived") {
            return Err(AppError::bad_request("项目状态无效"));
        }
    }
    let affected = sqlx::query(
        "UPDATE pent_projects
         SET name=$2, description=$3, asset_scope=$4, status=COALESCE($5,status), updated_at=now()
         WHERE id=$1 AND tenant_id=$6 AND ($7 OR created_by=$8) AND deleted=0",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(&asset_scope)
    .bind(status)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("项目不存在"));
    }
    get_project(State(s), Path(id), user).await
}

async fn delete_project(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_projects SET deleted=1, updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("项目不存在或已删除"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted": id}))))
}

async fn list_tasks(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) || jsonb_build_object(
             'status', COALESCE(
                 (SELECT r.status FROM pent_runs r
                  WHERE r.task_id=t.id
                  ORDER BY r.created_at DESC LIMIT 1),
                 CASE WHEN t.status='running' THEN 'cancelled' ELSE t.status END
             ),
             'latest_run_id',
             (SELECT r.id FROM pent_runs r
              WHERE r.task_id=t.id ORDER BY r.created_at DESC LIMIT 1)
         )
         FROM pent_tasks t
         WHERE tenant_id=$1 AND ($2 OR created_by=$3) AND deleted=0
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tasks t
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn create_task(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let id = Uuid::new_v4().to_string();
    let name = body["name"].as_str().unwrap_or("untitled");
    let desc = body.get("description").and_then(|v| v.as_str());
    let priority = body["priority"].as_str().unwrap_or("medium");
    let pid = body["project_id"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("请选择所属项目"))?;
    ensure_project_access(&s.pool, pid, &user).await?;
    let project_scope = sqlx::query_scalar::<_, Value>(
        "SELECT asset_scope FROM pent_projects WHERE id=$1 AND tenant_id=$2 AND deleted=0",
    )
    .bind(pid)
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    let target = normalize_confirmed_scope(project_scope, &user.user_id)?;
    sqlx::query("INSERT INTO pent_tasks(id,tenant_id,name,description,target_scope,priority,project_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(&id).bind(tenant_id).bind(name).bind(desc).bind(&target).bind(priority).bind(pid).bind(&user.user_id)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"任务创建成功"}),
    )))
}
async fn delete_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query("UPDATE pent_tasks SET deleted=1,updated_at=now() WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0")
        .bind(&id).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("任务不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted":id}))))
}

async fn cancel_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    sqlx::query(
        "UPDATE pent_tasks SET status='cancelled',updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"任务已取消"}))))
}

/// Extract an optional trimmed non-empty query parameter.
fn opt_param<'a>(
    params: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Option<&'a str> {
    params
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn list_runs(
    State(s): State<PentState>,
    user: CurrentUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let status = opt_param(&params, "status");
    let project_id = opt_param(&params, "project_id");
    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(1, 500);
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
            'id', r.id, 'task_id', r.task_id, 'task_name', tk.name,
            'status', r.status, 'session_id', r.session_id, 'worker_id', r.worker_id,
            'project_id', r.project_id, 'project_name', p.name,
            'objective', r.objective_snapshot,
            'scope_snapshot', r.scope_snapshot,
            'targets', COALESCE(r.scope_snapshot->'targets', '[]'::jsonb),
            'finding_count',
                (SELECT count(*) FROM pent_findings f WHERE f.run_id = r.id),
            'tool_call_count',
                (SELECT count(*) FROM pent_tool_calls c WHERE c.run_id = r.id),
            'failure_code', r.failure_code, 'failure_message', r.failure_message,
            'started_at', r.started_at, 'completed_at', r.completed_at,
            'created_at', r.created_at, 'updated_at', r.updated_at
         )
         FROM pent_runs r
         LEFT JOIN pent_tasks tk ON tk.id = r.task_id
         LEFT JOIN pent_projects p ON p.id = r.project_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)
           AND ($4::text IS NULL OR r.status = $4)
           AND ($5::text IS NULL OR r.project_id = $5)
         ORDER BY r.created_at DESC LIMIT $6",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(status)
    .bind(project_id)
    .bind(limit)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
            'id', r.id, 'task_id', r.task_id, 'task_name', tk.name,
            'status', r.status, 'session_id', r.session_id, 'worker_id', r.worker_id,
            'project_id', r.project_id, 'project_name', p.name,
            'objective', r.objective_snapshot,
            'scope_snapshot', r.scope_snapshot,
            'targets', COALESCE(r.scope_snapshot->'targets', '[]'::jsonb),
            'finding_count',
                (SELECT count(*) FROM pent_findings f WHERE f.run_id = r.id),
            'tool_call_count',
                (SELECT count(*) FROM pent_tool_calls c WHERE c.run_id = r.id),
            'failure_code', r.failure_code, 'failure_message', r.failure_message,
            'started_at', r.started_at, 'completed_at', r.completed_at,
            'created_at', r.created_at, 'updated_at', r.updated_at
         )
         FROM pent_runs r
         LEFT JOIN pent_tasks tk ON tk.id = r.task_id
         LEFT JOIN pent_projects p ON p.id = r.project_id
         WHERE r.id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn get_run_plan(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::ensure_run_access(&s.pool, &id, &user).await?;
    let plan = sqlx::query_scalar::<_, Value>(
        "SELECT graph FROM pent_plan_versions WHERE run_id=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(&id).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?
    .unwrap_or_else(|| json!({"run_id":id,"version":0,"nodes":[]}));
    Ok(Json(ApiResponse::new(plan)))
}
async fn create_run(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let id = Uuid::new_v4().to_string();
    let task_id = body["task_id"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("task_id required"))?;
    let affected = sqlx::query(
        "INSERT INTO pent_runs(
             id,task_id,tenant_id,user_id,project_id,status,
             objective_snapshot,scope_snapshot,policy_snapshot,budget_snapshot
         )
         SELECT $1,$2,t.tenant_id,$3,t.project_id,'queued',
                t.description,t.target_scope,
                jsonb_build_object('tool_profile_id',t.tool_profile_id,'scope_confirmed',true),
                '{}'::jsonb
         FROM pent_tasks t
         WHERE t.id=$2 AND t.tenant_id=$4 AND ($5 OR t.created_by=$3) AND t.deleted=0
           AND (
               t.target_scope->>'confirmed'='true'
               OR (
                   t.target_scope ? 'confirmed_at'
                   AND t.target_scope ? 'confirmed_by'
               )
           )
           AND jsonb_array_length(t.target_scope->'targets') > 0",
    )
    .bind(&id)
    .bind(task_id)
    .bind(&user.user_id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request(
            "任务不存在、无权访问或未确认目标范围",
        ));
    }
    // NATS: notify workers that a new run is available
    if let Some(ref nats) = s.nats {
        let payload =
            serde_json::to_vec(&json!({"run_id": id, "task_id": task_id})).unwrap_or_default();
        nats.publish("penta.run.requested", &payload).await;
    }
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"Run 创建成功"}),
    )))
}
async fn create_batch_runs(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let project_id = body
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("请选择所属项目"))?;
    ensure_project_access(&s.pool, project_id, &user).await?;

    let targets: Vec<String> = body
        .get("targets")
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if targets.is_empty() {
        return Err(AppError::bad_request("至少需要一个目标"));
    }
    // Dedup preserving order, cap to avoid runaway enqueue.
    let mut seen = std::collections::HashSet::new();
    let targets: Vec<String> = targets
        .into_iter()
        .filter(|target| seen.insert(target.to_owned()))
        .take(200)
        .collect();
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("批量测试");
    let priority = body
        .get("priority")
        .and_then(Value::as_str)
        .unwrap_or("medium");
    let objective = body
        .get("objective")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("对目标进行授权安全测试");

    let mut created = Vec::with_capacity(targets.len());
    for target in &targets {
        let task_id = Uuid::new_v4().to_string();
        let run_id = Uuid::new_v4().to_string();
        let scope = normalize_confirmed_scope(json!([target]), &user.user_id)?;
        let mut tx = s.pool.begin().await.map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query(
            "INSERT INTO pent_tasks(
                 id, tenant_id, name, description, target_scope, priority, project_id, created_by
             ) VALUES($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&task_id)
        .bind(tenant_id)
        .bind(format!("{name} - {target}"))
        .bind(objective)
        .bind(&scope)
        .bind(priority)
        .bind(project_id)
        .bind(&user.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query(
            "INSERT INTO pent_runs(
                 id,task_id,tenant_id,user_id,project_id,status,
                 objective_snapshot,scope_snapshot,policy_snapshot,budget_snapshot
             ) VALUES($1,$2,$3,$4,$5,'queued',$6,$7,$8,'{}'::jsonb)",
        )
        .bind(&run_id)
        .bind(&task_id)
        .bind(tenant_id)
        .bind(&user.user_id)
        .bind(project_id)
        .bind(objective)
        .bind(&scope)
        .bind(json!({"tool_profile_id": Value::Null, "scope_confirmed": true}))
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query(
            "UPDATE pent_tasks SET status='running', updated_at=now() WHERE id=$1",
        )
        .bind(&task_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        tx.commit().await.map_err(|e| AppError::internal(e.to_string()))?;
        if let Some(ref nats) = s.nats {
            let payload =
                serde_json::to_vec(&json!({"run_id": run_id, "task_id": task_id})).unwrap_or_default();
            nats.publish("penta.run.requested", &payload).await;
        }
        created.push(json!({"target": target, "task_id": task_id, "run_id": run_id}));
    }
    Ok(Json(ApiResponse::new(json!({
        "name": name,
        "total": created.len(),
        "runs": created,
    }))))
}
async fn cancel_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_runs
         SET cancel_requested_at=COALESCE(cancel_requested_at,now()),
             status='cancelled',
             completed_at=COALESCE(completed_at,now()),
             updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR user_id=$4)
           AND status IN ('queued','running','cancelling','waiting_approval')",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("Run 不存在或当前状态不能取消"));
    }
    sqlx::query(
        "UPDATE pent_tasks t SET status='cancelled',updated_at=now()
         FROM pent_runs r WHERE r.id=$1 AND t.id=r.task_id",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE pent_agent_sessions
         SET active_run_id=NULL,updated_at=now()
         WHERE active_run_id=$1",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE pent_approvals SET status='cancelled',updated_at=now()
         WHERE run_id=$1 AND status='pending'",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"Run 已取消"}))))
}

async fn pause_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_runs SET status='paused',cancel_requested_at=now(),worker_id=NULL,
                lease_expires_at=NULL,updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR user_id=$4)
           AND status IN ('queued','running','waiting_approval')",
    )
    .bind(&id).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.rows_affected();
    if affected == 0 { return Err(AppError::bad_request("Run 不存在或当前状态不能暂停")); }
    append_run_event(&s.pool, &id, "run.interrupted", json!({"mode":"pause","requested_by":user.user_id})).await.map_err(AppError::internal)?;
    Ok(Json(ApiResponse::new(json!({"message":"Run 已暂停；检查点和 Evidence 已保留"}))))
}

async fn resume_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_runs SET status='queued',cancel_requested_at=NULL,completed_at=NULL,
                failure_code=NULL,failure_message=NULL,next_attempt_at=now(),updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR user_id=$4) AND status='paused'",
    )
    .bind(&id).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.rows_affected();
    if affected == 0 { return Err(AppError::bad_request("Run 不存在或当前状态不能恢复")); }
    sqlx::query("UPDATE pent_agent_sessions SET status='running',active_run_id=$1,updated_at=now() WHERE id=(SELECT session_id FROM pent_runs WHERE id=$1)")
        .bind(&id).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    append_run_event(&s.pool, &id, "run.resumed", json!({"source":"checkpoint","requested_by":user.user_id})).await.map_err(AppError::internal)?;
    if let Some(ref nats) = s.nats {
        let payload = serde_json::to_vec(&json!({"run_id":id,"resume":true})).unwrap_or_default();
        nats.publish("penta.run.requested", &payload).await;
    }
    Ok(Json(ApiResponse::new(json!({"message":"Run 已从检查点重新入队"}))))
}

async fn delete_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let (status, task_id) = sqlx::query_as::<_, (String, String)>(
        "SELECT status,task_id FROM pent_runs
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR user_id=$4)
         FOR UPDATE",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("Run 不存在"))?;

    if !matches!(
        status.as_str(),
        "completed" | "failed" | "cancelled" | "cancelling"
    ) {
        return Err(AppError::bad_request(
            "只能删除已完成、失败、已取消或正在取消的 Run",
        ));
    }

    sqlx::query("UPDATE pent_agent_sessions SET active_run_id=NULL,updated_at=now() WHERE active_run_id=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;

    for table in [
        "pent_approvals",
        "pent_artifacts",
        "pent_context_snapshots",
        "pent_conversation_turns",
        "pent_evidence",
        "pent_findings",
        "pent_http_traffic",
        "pent_metacognitive_snapshots",
        "pent_model_calls",
        "pent_plan_deltas",
        "pent_plan_versions",
        "pent_pre_reflections",
        "pent_reflections",
        "pent_run_events",
        "pent_run_instructions",
        "pent_subtasks",
        "pent_tool_calls",
        "pent_tool_feedback",
        "pent_tool_route_decisions",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE run_id=$1"))
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
    }

    sqlx::query("DELETE FROM pent_runs WHERE id=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE pent_tasks SET status=COALESCE(
             (SELECT status FROM pent_runs WHERE task_id=$1 ORDER BY created_at DESC LIMIT 1),
             'cancelled'
         ),updated_at=now()
         WHERE id=$1",
    )
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    tx.commit()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;

    Ok(Json(ApiResponse::new(json!({"message":"Run 已删除"}))))
}
async fn get_run_events(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(e) FROM pent_run_events e JOIN pent_runs r ON r.id=e.run_id
         WHERE e.run_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4) ORDER BY e.sequence",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_tool_calls(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(c) FROM pent_tool_calls c JOIN pent_runs r ON r.id=c.run_id
         WHERE c.run_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4) ORDER BY c.created_at",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

// ── Worker executor with AI agent and lease renewal ─────────────────────

/// Background worker: picks up queued runs, calls AI for analysis, manages lease.
// ── Host tool auto-install ──────────────────────────────────────────────

async fn ensure_host_tools() {
    use crate::infrastructure::tool_lifecycle;

    let tools = tool_lifecycle::all_host_tools();
    let mut healthy = 0usize;
    let mut installed = 0usize;
    let mut failed = Vec::new();

    for lifecycle in &tools {
        if lifecycle.is_installed().await {
            healthy += 1;
            continue;
        }
        tracing::warn!(
            tool = lifecycle.tool_name,
            path = lifecycle.binary_path,
            "host tool missing; attempting install"
        );
        match lifecycle.install().await {
            Ok(()) => {
                tracing::info!(tool = lifecycle.tool_name, "installed successfully");
                installed += 1;
            }
            Err(error) => {
                tracing::error!(
                    tool = lifecycle.tool_name,
                    error = %error,
                    "install failed; tool will not be available"
                );
                failed.push(lifecycle.tool_name);
            }
        }
    }

    tracing::info!(
        healthy,
        installed,
        failed = ?failed,
        total = tools.len(),
        "host tool startup check complete"
    );
}

pub async fn start_worker(pool: PgPool, worker_id: String, lease_ttl: i64) {
    if let Err(error) = register_worker(&pool, &worker_id).await {
        tracing::warn!(worker=%worker_id, error=%error, "worker registration failed");
    }
    tokio::spawn(ensure_host_tools());
    let maintenance_pool = pool.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            if let Err(error) = maintenance_scanner(&maintenance_pool).await {
                tracing::warn!(error=%error, "Pent maintenance scanner failed");
            }
        }
    });
    let heartbeat_pool = pool.clone();
    let heartbeat_worker = worker_id.clone();
    tokio::spawn(async move {
        loop {
            if let Err(error) = heartbeat_worker_record(&heartbeat_pool, &heartbeat_worker).await {
                tracing::warn!(worker=%heartbeat_worker, error=%error, "worker heartbeat failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
    tokio::spawn(async move {
        loop {
            if let Err(e) = process_one_run(&pool, &worker_id, lease_ttl).await {
                tracing::warn!(worker=%worker_id, error=%e, "worker error");
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}

async fn register_worker(pool: &PgPool, worker_id: &str) -> Result<(), String> {
    let capacity: i32 = std::env::var("PENTA_WORKER_CAPACITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .clamp(1, 100);
    sqlx::query(
        "INSERT INTO pent_workers(worker_id,capacity,status,last_heartbeat,registered_at)
         VALUES($1,$2,'active',now(),now())
         ON CONFLICT(worker_id) DO UPDATE SET
             capacity=EXCLUDED.capacity,status='active',last_heartbeat=now()",
    )
    .bind(worker_id)
    .bind(capacity)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    tracing::info!(worker_id, capacity, "worker registered");
    Ok(())
}

async fn heartbeat_worker_record(pool: &PgPool, worker_id: &str) -> Result<(), String> {
    sqlx::query("UPDATE pent_workers SET status='active',last_heartbeat=now() WHERE worker_id=$1")
        .bind(worker_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn maintenance_scanner(pool: &PgPool) -> Result<(), String> {
    recovery_scanner(pool).await?;
    let expired = sqlx::query(
        "WITH expired AS (
             UPDATE pent_approvals
             SET status='expired',updated_at=now()
             WHERE status='pending' AND expires_at<=now()
             RETURNING run_id
         )
         UPDATE pent_runs r
         SET status='failed',failure_code='approval_expired',
             failure_message='高风险工具审批已过期',completed_at=now(),
             lease_expires_at=NULL,worker_id=NULL,updated_at=now()
         FROM expired
         WHERE r.id=expired.run_id AND r.status='waiting_approval'",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    sqlx::query(
        "UPDATE pent_workers SET status='offline'
         WHERE status='active' AND last_heartbeat<now()-interval '30 seconds'",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    if expired > 0 {
        tracing::info!(count = expired, "expired Pent approvals");
    }
    Ok(())
}

async fn recovery_scanner(pool: &PgPool) -> Result<(), String> {
    let terminal = sqlx::query(
        "UPDATE pent_runs r
         SET status='failed',failure_code='lease_exhausted',
             failure_message='Worker 租约连续过期且已达到最大重试次数',
             completed_at=now(),worker_id=NULL,lease_expires_at=NULL,updated_at=now()
         WHERE r.status='running' AND r.lease_expires_at<now()
           AND r.retry_count>=COALESCE(
               (SELECT max_retries_per_run FROM pent_runtime_policies WHERE tenant_id=r.tenant_id),
               (SELECT max_retries_per_run FROM pent_runtime_policies WHERE tenant_id=0),
               3
           )",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let retried = sqlx::query(
        "UPDATE pent_runs r
         SET status='queued',retry_count=retry_count+1,
             next_attempt_at=now()+(
                 LEAST(3600,COALESCE(
                     (SELECT retry_base_delay_seconds FROM pent_runtime_policies WHERE tenant_id=r.tenant_id),
                     (SELECT retry_base_delay_seconds FROM pent_runtime_policies WHERE tenant_id=0),
                     15
                 )*power(2,retry_count))::text||' seconds'
             )::interval,
             failure_code='lease_expired',failure_message='Worker 租约过期，已重新排队',
             worker_id=NULL,lease_expires_at=NULL,updated_at=now()
         WHERE r.status='running' AND r.lease_expires_at<now()",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    if terminal + retried > 0 {
        tracing::info!(retried, terminal, "recovered expired Pent run leases");
    }
    Ok(())
}

// ── M2-F12: Distributed scan nodes ────────────────────────────────────

async fn list_nodes(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'worker_id', worker_id,
             'status', status,
             'capacity', capacity,
             'last_heartbeat', last_heartbeat,
             'registered_at', registered_at
         ) FROM pent_workers ORDER BY registered_at DESC",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let _ = &user;
    Ok(Json(ApiResponse::new(rows)))
}

// ── Metrics ─────────────────────────────────────────────────────────────

async fn metrics(
    State(s): State<PentState>,
    user: CurrentUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let tenant_wide = security::tenant_wide(&user);
    async fn cnt(
        pool: &PgPool,
        status: &str,
        tenant_id: i64,
        tenant_wide: bool,
        user_id: &str,
    ) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_runs
             WHERE status=$1 AND tenant_id=$2 AND ($3 OR user_id=$4)",
        )
        .bind(status)
        .bind(tenant_id)
        .bind(tenant_wide)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    }
    async fn acnt(
        pool: &PgPool,
        status: &str,
        tenant_id: i64,
        tenant_wide: bool,
        user_id: &str,
    ) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_approvals a
             JOIN pent_runs r ON r.id=a.run_id
             WHERE a.status=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)",
        )
        .bind(status)
        .bind(tenant_id)
        .bind(tenant_wide)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    }
    let (
        queued,
        running,
        completed,
        failed,
        cancelled,
        waiting,
        sessions,
        findings,
        tools,
        tool_calls,
    ) = tokio::join!(
        cnt(&s.pool, "queued", tenant_id, tenant_wide, &user.user_id),
        cnt(&s.pool, "running", tenant_id, tenant_wide, &user.user_id),
        cnt(&s.pool, "completed", tenant_id, tenant_wide, &user.user_id),
        cnt(&s.pool, "failed", tenant_id, tenant_wide, &user.user_id),
        cnt(&s.pool, "cancelled", tenant_id, tenant_wide, &user.user_id),
        acnt(&s.pool, "pending", tenant_id, tenant_wide, &user.user_id),
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_agent_sessions
             WHERE tenant_id=$1 AND ($2 OR created_by=$3)"
        )
        .bind(tenant_id)
        .bind(tenant_wide)
        .bind(&user.user_id)
        .fetch_one(&s.pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_findings f
             JOIN pent_runs r ON r.id=f.run_id
             WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)"
        )
        .bind(tenant_id)
        .bind(tenant_wide)
        .bind(&user.user_id)
        .fetch_one(&s.pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_tool_definitions WHERE lifecycle_status='active'"
        )
        .fetch_one(&s.pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_tool_calls c
             JOIN pent_runs r ON r.id=c.run_id
             WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)"
        )
        .bind(tenant_id)
        .bind(tenant_wide)
        .bind(&user.user_id)
        .fetch_one(&s.pool),
    );
    let findings = findings.unwrap_or(0);
    let tools = tools.unwrap_or(0);
    let calls = tool_calls.unwrap_or(0);
    let is_prometheus = params.get("format").map(|v| v.as_str()) == Some("prometheus");
    if is_prometheus {
        let mut out = String::new();
        out.push_str(&format!("# HELP pent_runs_total Run count by status\n"));
        out.push_str(&format!("# TYPE pent_runs_total gauge\n"));
        out.push_str(&format!("pent_runs_total{{status=\"queued\"}} {}\n", queued));
        out.push_str(&format!("pent_runs_total{{status=\"running\"}} {}\n", running));
        out.push_str(&format!("pent_runs_total{{status=\"completed\"}} {}\n", completed));
        out.push_str(&format!("pent_runs_total{{status=\"failed\"}} {}\n", failed));
        out.push_str(&format!("pent_runs_total{{status=\"cancelled\"}} {}\n", cancelled));
        out.push_str(&format!("# HELP pent_approvals_pending Pending approval count\n"));
        out.push_str(&format!("# TYPE pent_approvals_pending gauge\n"));
        out.push_str(&format!("pent_approvals_pending {}\n", waiting));
        out.push_str(&format!("# HELP pent_sessions_total Active agent sessions\n"));
        out.push_str(&format!("# TYPE pent_sessions_total gauge\n"));
        out.push_str(&format!("pent_sessions_total {}\n", sessions.unwrap_or(0)));
        out.push_str(&format!("# HELP pent_findings_total Total findings\n"));
        out.push_str(&format!("# TYPE pent_findings_total gauge\n"));
        out.push_str(&format!("pent_findings_total {}\n", findings));
        out.push_str(&format!("# HELP pent_tools_active Active tool count\n"));
        out.push_str(&format!("# TYPE pent_tools_active gauge\n"));
        out.push_str(&format!("pent_tools_active {}\n", tools));
        out.push_str(&format!("# HELP pent_tool_calls_total Total tool calls\n"));
        out.push_str(&format!("# TYPE pent_tool_calls_total counter\n"));
        out.push_str(&format!("pent_tool_calls_total {}\n", calls));
        let success = if completed + failed > 0 {
            completed as f64 / (completed + failed) as f64
        } else { 0.0 };
        out.push_str(&format!("# HELP pent_success_rate Run success rate\n"));
        out.push_str(&format!("# TYPE pent_success_rate gauge\n"));
        out.push_str(&format!("pent_success_rate {:.4}\n", success));
        return Ok(axum::response::Response::builder()
            .header("content-type", "text/plain; version=0.0.4")
            .body(axum::body::Body::from(out))
            .unwrap());
    }
    Ok(axum::response::Response::builder()
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({
                "runs": {"queued":queued,"running":running,"completed":completed,"failed":failed,"cancelled":cancelled},
                "approvals_pending": waiting,
                "sessions": sessions.unwrap_or(0),
                "findings": findings,
                "tools": tools,
                "tool_calls": calls,
                "worker_count": 1,
                "success_rate": if completed + failed > 0 { format!("{:.0}%", completed as f64 / (completed + failed) as f64 * 100.0) } else { "N/A".to_string() }
            }).to_string(),
        ))
        .unwrap())
}

async fn tools_overview(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let tenant_wide = security::tenant_wide(&user);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pent_tool_definitions WHERE lifecycle_status='active'",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap_or(0);
    let total_calls: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pent_tool_calls c
         JOIN pent_runs r ON r.id=c.run_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)",
    )
    .bind(tenant_id)
    .bind(tenant_wide)
    .bind(&user.user_id)
    .fetch_one(&s.pool)
    .await
    .unwrap_or(0);
    let success_rate = if total_calls > 0 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pent_tool_calls c
             JOIN pent_runs r ON r.id=c.run_id
             WHERE c.exit_code=0 AND r.tenant_id=$1 AND ($2 OR r.user_id=$3)",
        )
        .bind(tenant_id)
        .bind(tenant_wide)
        .bind(&user.user_id)
        .fetch_one(&s.pool)
        .await
        .unwrap_or(0) as f64
            / total_calls as f64
            * 100.0
    } else {
        0.0
    };
    let top: Vec<Value> = sqlx::query_as::<_, (String, i64)>(
        "SELECT c.tool_name, count(*) as cnt
         FROM pent_tool_calls c
         JOIN pent_runs r ON r.id=c.run_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)
         GROUP BY c.tool_name ORDER BY cnt DESC LIMIT 5",
    )
    .bind(tenant_id)
    .bind(tenant_wide)
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(name, cnt)| json!({"name":name,"count":cnt}))
    .collect();
    Ok(Json(ApiResponse::new(
        json!({"total_tools":total,"total_calls":total_calls,"success_rate": format!("{:.0}%", success_rate),"healthy_tools":total,"top_tools":top}),
    )))
}


async fn process_one_run(pool: &PgPool, worker_id: &str, lease_ttl: i64) -> Result<(), String> {
    let Some(claim) = runtime_policy::claim(pool, worker_id, lease_ttl).await? else {
        return Ok(());
    };
    let lease_pool = pool.clone();
    let lease_run_id = claim.id.clone();
    let lease_worker_id = worker_id.to_string();
    let renewal_seconds = (claim.lease_ttl_seconds / 3).max(10) as u64;
    let lease_ttl_seconds = claim.lease_ttl_seconds;
    let lease_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(renewal_seconds)).await;
            let updated = sqlx::query(
                "UPDATE pent_runs
                 SET lease_expires_at=now()+($3||' seconds')::interval,updated_at=now()
                 WHERE id=$1 AND worker_id=$2 AND status='running'",
            )
            .bind(&lease_run_id)
            .bind(&lease_worker_id)
            .bind(lease_ttl_seconds)
            .execute(&lease_pool)
            .await
            .map(|result| result.rows_affected())
            .unwrap_or(0);
            if updated == 0 {
                break;
            }
        }
    });
    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(claim.max_duration_seconds.max(1) as u64),
        execute_claimed_run(pool, worker_id, &claim),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err("RUN_INCOMPLETE: run time budget exhausted".into()),
    };
    lease_task.abort();
    if let Err(error) = result {
        handle_run_failure(pool, &claim, &error).await?;
        return Err(error);
    }
    Ok(())
}

async fn execute_claimed_run(
    pool: &PgPool,
    worker_id: &str,
    claim: &runtime_policy::ClaimedRun,
) -> Result<(), String> {
    let run_id = claim.id.clone();
    if run_cancel_requested(pool, &run_id).await? {
        return Err("RUN_CANCELLED".into());
    }
    // Every run executes through the session-driven conversation runtime.
    // Runs created outside the chat flow (e.g. from a task) get a synthetic
    // agent session attached so the autonomous tool loop, evidence gates and
    // reporting apply uniformly. The H-PER-M cognition engine code is kept but
    // is no longer dispatched.
    ensure_run_session(pool, &run_id).await?;
    let session_run = tokio::select! {
        result = crate::conversation::run_bridge::execute_session_run(pool, &run_id, worker_id) => result,
        _ = wait_for_run_cancellation(pool, &run_id) => return Err("RUN_CANCELLED".into()),
    };
    if session_run? {
        return Ok(());
    }
    Err(format!(
        "Run {run_id} 未能关联 Agent 会话，无法进入会话运行时"
    ))
}

/// Attach a synthetic agent session to a run created outside the chat flow so
/// the worker can execute it through the conversation runtime.
async fn ensure_run_session(pool: &PgPool, run_id: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let run: Option<(
        i64,
        Option<String>,
        String,
        Option<String>,
        Value,
        Value,
        Value,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT tenant_id, project_id, user_id, objective_snapshot, scope_snapshot,
                policy_snapshot, budget_snapshot, session_id
         FROM pent_runs WHERE id=$1 FOR UPDATE",
    )
    .bind(run_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some((tenant_id, project_id, user_id, objective, scope, policy, budget, session_id)) = run
    else {
        return Err(format!("Run {run_id} 不存在"));
    };
    if session_id.is_some() {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(());
    }
    let objective = objective
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("安全测试运行 {run_id}"));
    let new_session_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pent_agent_sessions(
             id, tenant_id, project_id, objective, constraints, budget,
             scope_proposal, confirmed_scope_snapshot, status, active_run_id, created_by
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$7,'running',$8,$9)",
    )
    .bind(&new_session_id)
    .bind(tenant_id)
    .bind(&project_id)
    .bind(&objective)
    .bind(&policy)
    .bind(&budget)
    .bind(&scope)
    .bind(run_id)
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE pent_runs SET session_id=$2, updated_at=now() WHERE id=$1")
        .bind(run_id)
        .bind(&new_session_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn wait_for_run_cancellation(pool: &PgPool, run_id: &str) {
    loop {
        if run_cancel_requested(pool, run_id).await.unwrap_or(true) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

pub(crate) async fn run_cancel_requested(pool: &PgPool, run_id: &str) -> Result<bool, String> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM pent_runs
             WHERE id=$1 AND (cancel_requested_at IS NOT NULL OR status IN ('cancelling','cancelled'))
         )",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())
}

async fn handle_run_failure(
    pool: &PgPool,
    claim: &runtime_policy::ClaimedRun,
    error: &str,
) -> Result<(), String> {
    let cancelled = error == "RUN_CANCELLED" || run_cancel_requested(pool, &claim.id).await?;
    let incomplete = error.starts_with("RUN_INCOMPLETE:") || error.starts_with("RUN_BLOCKED:");
    let permanent = cancelled
        || incomplete
        || error.contains("缺少已确认")
        || error.contains("审批已拒绝")
        || error.contains("目标范围");
    let (status, failure_code) = if cancelled {
        ("cancelled", "cancelled")
    } else if incomplete {
        ("blocked", "incomplete")
    } else if permanent {
        ("failed", "permanent_failure")
    } else {
        ("queued", "transient_failure")
    };
    let affected = sqlx::query(
        "UPDATE pent_runs
         SET status=CASE
                 WHEN $2='queued' AND retry_count<$5 THEN 'queued'
                 WHEN $2='queued' THEN 'failed'
                 ELSE $2
             END,
             retry_count=CASE WHEN $2='queued' AND retry_count<$5 THEN retry_count+1 ELSE retry_count END,
             next_attempt_at=CASE
                 WHEN $2='queued' AND retry_count<$5
                 THEN now()+(LEAST($4*power(2,retry_count),3600)::text||' seconds')::interval
                 ELSE next_attempt_at
             END,
             failure_code=CASE
                 WHEN $2='queued' AND retry_count>=$5 THEN 'retry_exhausted'
                 ELSE $3
             END,
             failure_message=$6,
             completed_at=CASE
                 WHEN $2<>'queued' OR retry_count>=$5 THEN now()
                 ELSE NULL
             END,
             worker_id=NULL,lease_expires_at=NULL,updated_at=now()
         WHERE id=$1 AND status IN ('running','failed','cancelling')",
    )
    .bind(&claim.id)
    .bind(status)
    .bind(failure_code)
    .bind(claim.retry_base_delay_seconds.max(1))
    .bind(claim.max_retries_per_run.max(0))
    .bind(if cancelled {
        "运行已由用户取消"
    } else {
        error
    })
    .execute(pool)
    .await
    .map_err(|db_error| db_error.to_string())?
    .rows_affected();
    if affected > 0 {
        sqlx::query(
            "UPDATE pent_agent_sessions s
             SET status=CASE WHEN r.status='queued' THEN 'running' ELSE r.status END,
                 active_run_id=CASE WHEN r.status='queued' THEN r.id ELSE NULL END,
                 updated_at=now()
             FROM pent_runs r
             WHERE r.id=$1 AND s.id=r.session_id",
        )
        .bind(&claim.id)
        .execute(pool)
        .await
        .map_err(|db_error| db_error.to_string())?;
        append_run_event(
            pool,
            &claim.id,
            if cancelled {
                "run.cancelled"
            } else if permanent {
                "run.failed"
            } else {
                "run.retry_scheduled"
            },
            json!({
                "failure_code": failure_code,
                "message": error
            }),
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn append_run_event(
    pool: &PgPool,
    run_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload)
         SELECT $1,$2,COALESCE(MAX(sequence),-1)+1,$3,now(),$4
         FROM pent_run_events WHERE run_id=$2",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

// ── HTTP Traffic ───────────────────────────────────────────────────────────

async fn traffic_list(
    State(s): State<PentState>,
    user: CurrentUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let page = params
        .get("page")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = params
        .get("page_size")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(20)
        .clamp(1, 200);
    let offset = (page - 1) * page_size;
    let run_id = opt_param(&params, "run_id");
    let project_id = opt_param(&params, "project_id");
    let method = opt_param(&params, "method");
    let source = opt_param(&params, "source");
    let url_contains = opt_param(&params, "url");
    let status_code = params
        .get("status_code")
        .and_then(|value| value.parse::<i32>().ok());
    let duration_max_ms = params
        .get("duration_max_ms")
        .and_then(|value| value.parse::<i64>().ok());

    let items = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'id', t.id, 'run_id', t.run_id, 'tool_call_id', t.tool_call_id,
             'tool_name', t.tool_name, 'method', t.method, 'url', t.url,
             'response_status', t.response_status, 'duration_ms', t.duration_ms,
             'replay_of', t.replay_of, 'created_at', t.created_at,
             'request_size_bytes', octet_length(coalesce(t.request_body,'')),
             'response_size_bytes', octet_length(coalesce(t.response_body,''))
         )
         FROM pent_http_traffic t
         JOIN pent_runs r ON r.id = t.run_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)
           AND ($4::text IS NULL OR t.run_id = $4)
           AND ($5::text IS NULL OR r.project_id = $5)
           AND ($6::text IS NULL OR upper(t.method) = upper($6))
           AND ($7::text IS NULL OR t.tool_name = $7)
           AND ($8::text IS NULL OR t.url ILIKE '%'||$8||'%')
           AND ($9::int IS NULL OR t.response_status = $9)
           AND ($10::bigint IS NULL OR t.duration_ms <= $10)
         ORDER BY t.created_at DESC
         LIMIT $11 OFFSET $12",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(run_id)
    .bind(project_id)
    .bind(method)
    .bind(source)
    .bind(url_contains)
    .bind(status_code)
    .bind(duration_max_ms)
    .bind(page_size)
    .bind(offset)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM pent_http_traffic t
         JOIN pent_runs r ON r.id = t.run_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)
           AND ($4::text IS NULL OR t.run_id = $4)
           AND ($5::text IS NULL OR r.project_id = $5)
           AND ($6::text IS NULL OR upper(t.method) = upper($6))
           AND ($7::text IS NULL OR t.tool_name = $7)
           AND ($8::text IS NULL OR t.url ILIKE '%'||$8||'%')
           AND ($9::int IS NULL OR t.response_status = $9)
           AND ($10::bigint IS NULL OR t.duration_ms <= $10)",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(run_id)
    .bind(project_id)
    .bind(method)
    .bind(source)
    .bind(url_contains)
    .bind(status_code)
    .bind(duration_max_ms)
    .fetch_one(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(ApiResponse::new(json!({
        "items": items,
        "total": total,
        "page": page,
        "page_size": page_size,
    }))))
}

// ── M2-F14: Cross-project global memory search ──────────────────────

async fn rmos_global_search(
    State(s): State<PentState>,
    user: CurrentUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let query = params.get("q").map(String::as_str).unwrap_or("");
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let tenant_id = security::tenant_id(&user)?;
    let rows = if query.is_empty() {
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(t) FROM pent_knowledge_documents t
             WHERE tenant_id=$1 AND ($2 OR created_by=$3)
             ORDER BY created_at DESC LIMIT $4",
        )
        .bind(tenant_id)
        .bind(security::tenant_wide(&user))
        .bind(&user.user_id)
        .bind(limit)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
    } else {
        // Full-text search across all projects in the tenant
        sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(t) || jsonb_build_object(
                 'rank', ts_rank(
                     to_tsvector('simple', coalesce(t.content,'')),
                     plainto_tsquery('simple', $1)
                 )
             )
             FROM pent_knowledge_documents t
             WHERE t.tenant_id=$2 AND ($3 OR t.created_by=$4)
               AND to_tsvector('simple', coalesce(t.content,'')) @@ plainto_tsquery('simple', $1)
             ORDER BY ts_rank(
                 to_tsvector('simple', coalesce(t.content,'')),
                 plainto_tsquery('simple', $1)
             ) DESC
             LIMIT $5",
        )
        .bind(&query)
        .bind(tenant_id)
        .bind(security::tenant_wide(&user))
        .bind(&user.user_id)
        .bind(limit)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
    };
    Ok(Json(ApiResponse::new(rows)))
}

// ── RMOS Memory ────────────────────────────────────────────────────────────
//
// Memory entries are written by the conversation runtime into
// `pent_knowledge_documents` with `source='conversation-memory'` and
// `tags = ["conversation-memory", <run_id>, <tool_name>]`. Tenancy is derived
// from the referenced run so cross-tenant reads/writes are impossible.

async fn rmos_memories(
    State(s): State<PentState>,
    user: CurrentUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let project_id = opt_param(&params, "project_id");
    let run_id = opt_param(&params, "run_id");
    let query = opt_param(&params, "q");
    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(200)
        .clamp(1, 500);

    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
             'id', kd.id::text,
             'title', kd.title,
             'memory_type', COALESCE(kd.answer_type, kd.doc_type),
             'source', kd.source,
             'run_id', r.id,
             'session_id', r.session_id,
             'project_id', r.project_id,
             'project_name', p.name,
             'tool_name', kd.tags->>2,
             'content', kd.content,
             'summary', left(coalesce(kd.content,''), 300),
             'tags', kd.tags,
             'feedback', CASE
                 WHEN kd.tags ? 'useful' THEN 'useful'
                 WHEN kd.tags ? 'useless' THEN 'useless'
                 ELSE NULL END,
             'created_at', kd.created_at,
             'updated_at', kd.updated_at
         )
         FROM pent_knowledge_documents kd
         JOIN pent_runs r ON r.id = kd.tags->>1
         LEFT JOIN pent_projects p ON p.id = r.project_id
         WHERE kd.source = 'conversation-memory'
           AND r.tenant_id=$1 AND ($2 OR r.user_id=$3)
           AND ($4::text IS NULL OR r.project_id = $4)
           AND ($5::text IS NULL OR r.id = $5)
           AND ($6::text IS NULL
                OR kd.title ILIKE '%'||$6||'%'
                OR kd.content ILIKE '%'||$6||'%')
         ORDER BY kd.created_at DESC LIMIT $7",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(project_id)
    .bind(run_id)
    .bind(query)
    .bind(limit)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn rmos_feedback(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let feedback = body["feedback"].as_str().unwrap_or("");
    let tag = match feedback {
        "useful" | "useless" => feedback,
        other => return Err(AppError::bad_request(format!("不支持的反馈类型: {other}"))),
    };
    let affected = sqlx::query(
        "UPDATE pent_knowledge_documents kd
         SET tags = COALESCE(kd.tags, '[]'::jsonb) || $2::jsonb,
             updated_at = now()
         FROM pent_runs r
         WHERE kd.id = $1::uuid
           AND kd.source = 'conversation-memory'
           AND r.id = kd.tags->>1
           AND r.tenant_id = $3 AND ($4 OR r.user_id = $5)",
    )
    .bind(&id)
    .bind(json!([tag]))
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("记忆不存在或无权访问"));
    }
    Ok(Json(ApiResponse::new(
        json!({"status":"ok","feedback":feedback}),
    )))
}

// ── Attack Graph ───────────────────────────────────────────────────────────

async fn attack_graph(
    State(s): State<PentState>,
    user: CurrentUser,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let project_id = opt_param(&params, "project_id");
    let findings: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT f.id, f.title, f.severity, f.target
         FROM pent_findings f
         JOIN pent_runs r ON r.id = f.run_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)
           AND ($4::text IS NULL OR r.project_id = $4)
         ORDER BY f.created_at DESC LIMIT 200",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(project_id)
    .fetch_all(&s.pool)
    .await
    .unwrap_or_default();

    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut seen_targets = std::collections::HashSet::new();
    let mut summary_critical = 0usize;
    let mut summary_high = 0usize;
    let mut summary_other = 0usize;

    for (id, title, severity, target) in &findings {
        let category = match severity.as_str() {
            "critical" => 0,
            "high" => 1,
            _ => 2,
        };
        let size = match severity.as_str() {
            "critical" => 35,
            "high" => 28,
            _ => 22,
        };
        match severity.as_str() {
            "critical" => summary_critical += 1,
            "high" => summary_high += 1,
            _ => summary_other += 1,
        }
        nodes.push(json!({
            "id": id,
            "name": title,
            "kind": "vulnerability",
            "severity": severity,
            "category": category,
            "symbolSize": size,
        }));
        let target_name = if target.trim().is_empty() {
            "未知目标"
        } else {
            target.as_str()
        };
        if !seen_targets.contains(target_name) {
            seen_targets.insert(target_name.to_string());
            nodes.push(json!({
                "id": target_name,
                "name": target_name,
                "kind": "asset",
                "severity": null,
                "category": 3,
                "symbolSize": 45,
                "symbol": "roundRect",
            }));
        }
        edges.push(json!({
            "source": target_name,
            "target": id,
            "kind": "exposes",
            "label": "发现漏洞",
        }));
    }
    Ok(Json(ApiResponse::new(json!({
        "nodes": nodes,
        "edges": edges,
        "summary": {
            "assets": seen_targets.len(),
            "vulnerabilities": findings.len(),
            "critical": summary_critical,
            "high": summary_high,
            "medium_low": summary_other,
        },
    }))))
}

// ── Platform Settings ──────────────────────────────────────────────────────

fn platform_section_defaults(section: &str) -> Option<Value> {
    Some(match section {
        "assets" => json!({"auto_discovery":true,"discovery_interval_minutes":60,"alive_check_enabled":true,"asset_retention_days":365,"duplicate_strategy":"merge","risk_sync_enabled":true}),
        "knowledge" => json!({"enabled":true,"base_path":"knowledge_base","embedding_model_config_id":0,"retrieval_top_k":5,"similarity_threshold":0.65,"sub_index_filter":"","multi_query_max_queries":4,"rerank_model_config_id":0,"prefetch_top_k":20,"max_context_chars":0,"max_context_tokens":0,"chunk_strategy":"markdown_then_recursive","request_timeout_seconds":120,"batch_size":64,"prefer_source_file":false,"sub_indexes":"","chunk_size":768,"chunk_overlap":50,"max_chunks_per_item":20,"max_rpm":100,"rate_limit_delay_ms":600,"max_retries":3,"retry_delay_ms":1000}),
        "c2" => json!({"enabled":true,"heartbeat_seconds":30,"offline_timeout_seconds":120,"max_concurrent_jobs":10,"tls_required":true,"allow_unregistered_nodes":false,"command_timeout_seconds":600}),
        "robots" => json!({"enabled":true,"max_active_agents":8,"default_role":"penetration_tester","auto_assign_tasks":true,"idle_timeout_minutes":30,"memory_enabled":true,"collaboration_enabled":true}),
        "terminal" => json!({"session_timeout_minutes":60,"max_sessions_per_user":5,"command_audit_enabled":true,"dangerous_command_confirmation":true,"file_transfer_enabled":true,"max_upload_mb":100,"default_shell":"bash"}),
        _ => return None,
    })
}

async fn get_robot_settings(State(s): State<PentState>, user: CurrentUser) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let stored = sqlx::query_scalar::<_, Option<Value>>("SELECT agent_config->'robot_channels' FROM pent_runtime_policies WHERE tenant_id IN ($1,0) ORDER BY (tenant_id=$1) DESC LIMIT 1")
        .bind(tenant_id).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.flatten().unwrap_or_else(|| json!({}));
    let channels = ["wechat","wecom","dingtalk","lark","telegram","slack","discord","qq"];
    Ok(Json(ApiResponse::new(Value::Array(channels.into_iter().map(|id| json!({"id":id,"enabled":stored[id]["enabled"].as_bool().unwrap_or(false),"app_id":stored[id]["app_id"].as_str().unwrap_or(""),"configured":stored[id]["secret"].as_str().is_some_and(|v| !v.is_empty())})).collect()))))
}

async fn update_robot_settings(State(s): State<PentState>, user: CurrentUser, Json(body): Json<Value>) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let old = sqlx::query_scalar::<_, Option<Value>>("SELECT agent_config->'robot_channels' FROM pent_runtime_policies WHERE tenant_id=$1").bind(tenant_id).fetch_optional(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.flatten().unwrap_or_else(|| json!({}));
    let rows = body["channels"].as_array().ok_or_else(|| AppError::bad_request("channels 必须是数组"))?;
    let mut saved = serde_json::Map::new();
    for row in rows { let id=row["id"].as_str().filter(|v| matches!(*v,"wechat"|"wecom"|"dingtalk"|"lark"|"telegram"|"slack"|"discord"|"qq")).ok_or_else(|| AppError::bad_request("不支持的机器人类型"))?; let input=row["secret"].as_str().unwrap_or(""); let secret=if input.is_empty(){old[id]["secret"].clone()}else{Value::String(infrastructure::secrets::encrypt_api_key(input))}; saved.insert(id.into(),json!({"enabled":row["enabled"].as_bool().unwrap_or(false),"app_id":row["app_id"].as_str().unwrap_or(""),"secret":secret})); }
    let config=Value::Object(saved);
    sqlx::query("INSERT INTO pent_runtime_policies(tenant_id,agent_config,updated_by) VALUES($1,jsonb_build_object('robot_channels',$2),$3) ON CONFLICT(tenant_id) DO UPDATE SET agent_config=pent_runtime_policies.agent_config || jsonb_build_object('robot_channels',$2),updated_by=$3,updated_at=now()")
        .bind(tenant_id).bind(&config).bind(&user.user_id).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    get_robot_settings(State(s),user).await
}

async fn get_platform_section(
    State(s): State<PentState>,
    Path(section): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let defaults = platform_section_defaults(&section)
        .ok_or_else(|| AppError::not_found("设置分类不存在"))?;
    let tenant_id = security::tenant_id(&user)?;
    let stored = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT agent_config->'platform_settings'->$2
         FROM pent_runtime_policies
         WHERE tenant_id IN ($1,0)
         ORDER BY (tenant_id=$1) DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(&section)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .flatten()
    .unwrap_or_else(|| json!({}));
    let merged = match (defaults, stored) {
        (Value::Object(mut base), Value::Object(values)) => {
            for (key, value) in values {
                if base.contains_key(&key) {
                    base.insert(key, value);
                }
            }
            Value::Object(base)
        }
        (base, _) => base,
    };
    Ok(Json(ApiResponse::new(merged)))
}

async fn update_platform_section(
    State(s): State<PentState>,
    Path(section): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let defaults = platform_section_defaults(&section)
        .ok_or_else(|| AppError::not_found("设置分类不存在"))?;
    let values = body
        .as_object()
        .ok_or_else(|| AppError::bad_request("设置内容必须是对象"))?;
    let allowed = defaults.as_object().expect("section defaults are objects");
    if values.keys().any(|key| !allowed.contains_key(key)) {
        return Err(AppError::bad_request("设置包含未知字段"));
    }
    let tenant_id = security::tenant_id(&user)?;
    sqlx::query(
        "INSERT INTO pent_runtime_policies(tenant_id,agent_config,updated_by)
         VALUES($1,jsonb_build_object('platform_settings',jsonb_build_object($2,$3)), $4)
         ON CONFLICT(tenant_id) DO UPDATE SET
           agent_config=pent_runtime_policies.agent_config || jsonb_build_object(
             'platform_settings',
             COALESCE(pent_runtime_policies.agent_config->'platform_settings','{}'::jsonb)
               || jsonb_build_object($2,$3)
           ),updated_by=$4,updated_at=now()",
    )
    .bind(tenant_id)
    .bind(&section)
    .bind(&body)
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(body)))
}

async fn get_model_bindings(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) || jsonb_build_object('inherited', t.tenant_id <> $1)
         FROM pent_model_role_bindings t
         WHERE t.tenant_id=$1 OR (
             t.tenant_id=0 AND NOT EXISTS (
                 SELECT 1 FROM pent_model_role_bindings own
                 WHERE own.tenant_id=$1 AND own.project_id=t.project_id
                   AND own.model_role=t.model_role
             )
         )
         ORDER BY model_role",
    )
    .bind(tenant_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn update_model_bindings(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let bindings = body["bindings"]
        .as_array()
        .ok_or_else(|| AppError::bad_request("bindings 必须是数组"))?;
    let mut tx = s.pool.begin().await.map_err(|e| AppError::internal(e.to_string()))?;
    for binding in bindings {
        let role = binding["model_role"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::bad_request("模型角色不能为空"))?;
        let project_id = binding["project_id"].as_str().unwrap_or("");
        let model_config_id = binding["model_config_id"]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(|| AppError::bad_request("模型配置 ID 必须大于 0"))?;
        let temperature = binding["temperature"].as_f64().unwrap_or(0.7);
        let max_tokens = binding["max_output_tokens"].as_i64().unwrap_or(4096);
        let timeout = binding["timeout_seconds"].as_i64().unwrap_or(120);
        if !(0.0..=2.0).contains(&temperature)
            || !(256..=131_072).contains(&max_tokens)
            || !(5..=3600).contains(&timeout)
        {
            return Err(AppError::bad_request("模型参数超出允许范围"));
        }
        sqlx::query(
            "INSERT INTO pent_model_role_bindings(
                 tenant_id,project_id,model_role,model_config_id,temperature,
                 max_output_tokens,timeout_seconds,enabled,revision,updated_at
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,1,now())
             ON CONFLICT(tenant_id,project_id,model_role) DO UPDATE SET
                 model_config_id=EXCLUDED.model_config_id,
                 temperature=EXCLUDED.temperature,
                 max_output_tokens=EXCLUDED.max_output_tokens,
                 timeout_seconds=EXCLUDED.timeout_seconds,
                 enabled=EXCLUDED.enabled,
                 revision=pent_model_role_bindings.revision+1,
                 updated_at=now()",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(role)
        .bind(model_config_id)
        .bind(temperature)
        .bind(max_tokens as i32)
        .bind(timeout as i32)
        .bind(binding["enabled"].as_bool().unwrap_or(true))
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    }
    tx.commit().await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"已保存"}))))
}

async fn get_runtime_policy(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let policy = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t)
             - 'approval_ttl_seconds' - 'auto_approve_low_risk' - 'require_approval_for'
             || jsonb_build_object('inherited', t.tenant_id <> $1)
         FROM pent_runtime_policies t
         WHERE tenant_id IN ($1, 0)
         ORDER BY CASE WHEN tenant_id=$1 THEN 0 ELSE 1 END
         LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .unwrap_or_else(|| json!({
        "max_concurrent_runs": 5,
        "lease_ttl_seconds": 120,
        "max_tool_timeout_seconds": 900,
        "max_retries_per_run": 3,
        "retry_base_delay_seconds": 15,
        "inherited": true
    }));
    Ok(Json(ApiResponse::new(policy)))
}

async fn update_runtime_policy(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let concurrent = body["max_concurrent_runs"].as_i64().unwrap_or(5);
    let lease = body["lease_ttl_seconds"].as_i64().unwrap_or(120);
    let timeout = body["max_tool_timeout_seconds"].as_i64().unwrap_or(900);
    let retries = body["max_retries_per_run"].as_i64().unwrap_or(3);
    let retry_delay = body["retry_base_delay_seconds"].as_i64().unwrap_or(15);
    if !(1..=100).contains(&concurrent)
        || !(30..=3600).contains(&lease)
        || !(10..=7200).contains(&timeout)
        || !(0..=20).contains(&retries)
        || !(1..=3600).contains(&retry_delay)
    {
        return Err(AppError::bad_request("运行策略参数超出允许范围"));
    }
    sqlx::query(
        "INSERT INTO pent_runtime_policies(
             tenant_id,max_concurrent_runs,lease_ttl_seconds,max_tool_timeout_seconds,
             max_retries_per_run,retry_base_delay_seconds,updated_by,updated_at
         ) VALUES($1,$2,$3,$4,$5,$6,$7,now())
         ON CONFLICT(tenant_id) DO UPDATE SET
             max_concurrent_runs=EXCLUDED.max_concurrent_runs,
             lease_ttl_seconds=EXCLUDED.lease_ttl_seconds,
             max_tool_timeout_seconds=EXCLUDED.max_tool_timeout_seconds,
             max_retries_per_run=EXCLUDED.max_retries_per_run,
             retry_base_delay_seconds=EXCLUDED.retry_base_delay_seconds,
             updated_by=EXCLUDED.updated_by,updated_at=now()",
    )
    .bind(tenant_id)
    .bind(concurrent as i32)
    .bind(lease as i32)
    .bind(timeout as i32)
    .bind(retries as i32)
    .bind(retry_delay as i32)
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"运行策略已保存"}))))
}

// ── SSE streaming for run events ──────────────────────────────────────────

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use std::convert::Infallible;

async fn run_events_sse(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    security::ensure_run_access(&s.pool, &id, &user).await?;
    let mut listener = sqlx::postgres::PgListener::connect_with(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    listener
        .listen("pent_run_events")
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let stream = futures_util::stream::unfold(
        (listener, s.pool.clone(), id, -1_i64),
        |(mut listener, pool, run_id, mut last_seq)| async move {
            loop {
                let row = sqlx::query_as::<_, (String, i64, String, Value)>(
                    "SELECT event_id,sequence,event_type,payload
                     FROM pent_run_events
                     WHERE run_id=$1 AND sequence>$2
                     ORDER BY sequence LIMIT 1",
                )
                .bind(&run_id)
                .bind(last_seq)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();
                if let Some((event_id, sequence, event_type, payload)) = row {
                    last_seq = sequence;
                    let data = json!({
                        "event_id": event_id,
                        "sequence": sequence,
                        "event_type": event_type,
                        "payload": payload
                    });
                    let event = Event::default().event(event_type).data(data.to_string());
                    return Some((Ok(event), (listener, pool, run_id, last_seq)));
                }
                if listener.recv().await.is_err() {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        },
    );
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ── Evidence, Findings & Artifacts ─────────────────────────────────────

async fn get_run_evidence(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT (to_jsonb(e)-'source') || jsonb_build_object(
                    'source_size_bytes',octet_length(e.source)
                )
         FROM pent_evidence e JOIN pent_runs r ON r.id=e.run_id
         WHERE e.run_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4) ORDER BY e.created_at",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_run_findings(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(f) FROM pent_findings f JOIN pent_runs r ON r.id=f.run_id
         WHERE f.run_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4) ORDER BY f.created_at DESC",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_run_artifacts(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(a) FROM pent_artifacts a JOIN pent_runs r ON r.id=a.run_id
         WHERE a.run_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4) ORDER BY a.created_at",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_evidence(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(e) FROM pent_evidence e JOIN pent_runs r ON r.id=e.run_id WHERE e.id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)")
        .bind(&id)
        .bind(tenant_id)
        .bind(security::tenant_wide(&user))
        .bind(&user.user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    // Filter sensitive fields from evidence content
    let mut sanitized = v.clone();
    if let Some(obj) = sanitized.as_object_mut() {
        if let Some(source) = obj.get("source").and_then(|s| s.as_str()) {
            obj.insert(
                "source".into(),
                Value::String(sanitize_sensitive_data(source)),
            );
        }
    }
    Ok(Json(ApiResponse::new(sanitized)))
}

async fn get_run_traffic(
    State(s): State<PentState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let offset = params
        .get("offset")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let status = params
        .get("status")
        .and_then(|value| value.parse::<i32>().ok());
    let method = params
        .get("method")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let url_contains = params
        .get("url_contains")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM (
             SELECT t.id, t.run_id, t.tool_call_id, t.tool_name, t.method, t.url,
                    t.response_status, t.duration_ms, t.replay_of, t.created_at,
                    octet_length(coalesce(t.response_body,'')) AS response_size_bytes
             FROM pent_http_traffic t JOIN pent_runs r ON r.id=t.run_id
             WHERE t.run_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)
               AND ($5::text IS NULL OR t.url ILIKE '%'||$5||'%')
               AND ($6::text IS NULL OR t.method=upper($6))
               AND ($7::int IS NULL OR t.response_status=$7)
             ORDER BY t.created_at DESC LIMIT $8 OFFSET $9
         ) t",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(url_contains)
    .bind(method)
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn get_traffic(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_http_traffic t JOIN pent_runs r ON r.id=t.run_id
         WHERE t.id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    let mut sanitized = v.clone();
    if let Some(obj) = sanitized.as_object_mut() {
        for field in ["request_body", "response_body"] {
            if let Some(text) = obj.get(field).and_then(Value::as_str) {
                obj.insert(field.into(), Value::String(sanitize_sensitive_data(text)));
            }
        }
        for field in ["request_headers", "response_headers"] {
            if let Some(headers) = obj.get(field).and_then(Value::as_object) {
                let redacted: serde_json::Map<String, Value> = headers
                    .iter()
                    .map(|(name, value)| {
                        let sensitive = matches!(
                            name.to_ascii_lowercase().as_str(),
                            "authorization"
                                | "proxy-authorization"
                                | "cookie"
                                | "set-cookie"
                                | "x-csrf-token"
                                | "x-api-key"
                        );
                        if sensitive {
                            (name.clone(), Value::String("***REDACTED***".into()))
                        } else {
                            (name.clone(), value.clone())
                        }
                    })
                    .collect();
                obj.insert(field.into(), Value::Object(redacted));
            }
        }
    }
    Ok(Json(ApiResponse::new(sanitized)))
}
// ── Project-scoped findings, traffic & vulnerabilities ──────────────

async fn get_project_findings(
    State(s): State<PentState>,
    Path(project_id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    // Reject unauthorized project access
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pent_projects WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0)",
    )
    .bind(&project_id).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .fetch_one(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    if !exists { return Err(AppError::bad_request("项目不存在或无权访问")); }
    // Use project_id directly when populated, fall back to run-level join
    let items = sqlx::query_scalar::<_, Value>(
        "SELECT row_to_json(t) FROM (
             SELECT f.* FROM pent_findings f
             WHERE f.project_id=$1 AND f.project_id IS NOT NULL
             UNION ALL
             SELECT f.* FROM pent_findings f
             JOIN pent_runs r ON r.id=f.run_id
             WHERE r.project_id=$1 AND f.project_id IS NULL AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)
             ORDER BY created_at DESC LIMIT 200
         ) t",
    )
    .bind(&project_id).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    // Severity summary
    let summary: Vec<(String, i64)> = sqlx::query_as(
        "SELECT severity, cnt FROM (
             SELECT COALESCE(f.severity,'info') as severity, count(*) as cnt FROM pent_findings f
             WHERE f.project_id=$1 AND f.project_id IS NOT NULL
             GROUP BY f.severity
             UNION ALL
             SELECT COALESCE(f.severity,'info') as severity, count(*) as cnt FROM pent_findings f
             JOIN pent_runs r ON r.id=f.run_id
             WHERE r.project_id=$1 AND f.project_id IS NULL AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)
             GROUP BY f.severity
         ) t",
    )
    .bind(&project_id).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .fetch_all(&s.pool).await.unwrap_or_default();
    let severity_summary: serde_json::Value = summary.into_iter().fold(json!({}), |mut acc, (sev, cnt)| {
        acc[&sev] = json!(cnt); acc
    });
    Ok(Json(ApiResponse::new(json!({"items": items, "severity_summary": severity_summary}))))
}

async fn get_project_traffic(
    State(s): State<PentState>,
    Path(project_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(100).clamp(1, 500);
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM (
             SELECT t.id, t.run_id, t.tool_name, t.method, t.url, t.response_status, t.duration_ms, t.created_at
             FROM pent_http_traffic t
             JOIN pent_runs r ON r.id=t.run_id
             WHERE r.project_id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)
             ORDER BY t.created_at DESC LIMIT $5
         ) t",
    )
    .bind(&project_id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .bind(limit)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn get_finding(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(f) FROM pent_findings f JOIN pent_runs r ON r.id=f.run_id WHERE f.id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)")
        .bind(&id)
        .bind(tenant_id)
        .bind(security::tenant_wide(&user))
        .bind(&user.user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}

async fn review_finding(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let decision = body
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(decision, "confirmed" | "false_positive") {
        return Err(AppError::bad_request(
            "decision 必须是 confirmed 或 false_positive",
        ));
    }
    let value = sqlx::query_scalar::<_, Value>(
        "UPDATE pent_findings f
         SET false_positive_status=$2,
             status=CASE WHEN $2='confirmed' THEN 'confirmed' ELSE 'false_positive' END,
             reviewed_by=$3,reviewed_at=now(),review_comment=$4,updated_at=now()
         FROM pent_runs r
         WHERE f.id=$1 AND r.id=f.run_id AND r.tenant_id=$5
           AND ($6 OR r.user_id=$3)
           AND jsonb_array_length(f.evidence_ids)>0
         RETURNING to_jsonb(f)",
    )
    .bind(&id)
    .bind(decision)
    .bind(&user.user_id)
    .bind(body.get("comment").and_then(Value::as_str))
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("Finding 不存在或缺少 Evidence"))?;
    Ok(Json(ApiResponse::new(value)))
}

async fn download_artifact(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<axum::response::Response, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT a.object_key, a.content_type, a.size_bytes FROM pent_artifacts a JOIN pent_runs r ON r.id=a.run_id WHERE a.id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let (key, content_type, _size) = row.ok_or_else(|| AppError::not_found("not found"))?;
    let store = infrastructure::object_store::ObjectStore::from_env();
    let (stored_type, bytes) = store
        .get_object(&key)
        .await
        .map_err(|error| AppError::internal(format!("产物存储读取失败：{error}")))?;
    let filename = key.rsplit('/').next().unwrap_or("artifact.bin");
    let content_type = if content_type.is_empty() {
        stored_type
    } else {
        content_type
    };
    Ok(axum::response::Response::builder()
        .header("content-type", content_type)
        .header(
            "content-disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .header("content-length", bytes.len())
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// Filter sensitive data (tokens, passwords, keys) from evidence text.
pub(crate) fn sanitize_sensitive_data(text: &str) -> String {
    let patterns: &[(&str, &str)] = &[
        ("Authorization", "Authorization: ***REDACTED***"),
        ("Cookie", "Cookie: ***REDACTED***"),
        ("Token", "Token: ***REDACTED***"),
        ("api_key", "api_key=***REDACTED***"),
        ("password", "password=***REDACTED***"),
        ("secret", "secret=***REDACTED***"),
    ];
    let mut result = text.to_string();
    for (key, replacement) in patterns {
        let lower = result.to_lowercase();
        if let Some(pos) = lower.find(&key.to_lowercase()) {
            let end = result[pos..]
                .find('\n')
                .map(|p| pos + p)
                .unwrap_or(result.len());
            if end - pos < 500 {
                result.replace_range(pos..end, replacement);
            }
        }
    }
    result
}
#[derive(serde::Serialize)]
pub(crate) struct ApiResponse<T: serde::Serialize> {
    code: i32,
    data: T,
    message: String,
}
impl<T: serde::Serialize> ApiResponse<T> {
    pub(crate) fn new(data: T) -> Self {
        Self {
            code: 0,
            data,
            message: "ok".into(),
        }
    }
}
