pub mod application;
pub mod cognition;
pub mod config;
pub mod conversation;
pub mod infrastructure;
pub(crate) mod report;
mod runtime_policy;
mod security;
mod session;
mod skills;
pub mod tooling;
pub mod transport;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    middleware::from_fn,
    response::IntoResponse,
    routing::{delete, get, post},
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
}

impl PentState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            conversation_tasks: Default::default(),
            nats: None,
        }
    }

    pub fn with_nats(mut self, nats: crate::transport::mq::NatsHandle) -> Self {
        self.nats = Some(nats);
        self
    }
}

pub fn routes(state: PentState) -> Router {
    let protected = Router::new()
        .route("/pent/projects", get(list_projects).post(create_project))
        .route("/pent/projects/{id}", get(get_project))
        .route("/pent/tasks", get(list_tasks).post(create_task))
        .route("/pent/tasks/{id}", get(get_task).delete(delete_task))
        .route("/pent/tasks/{id}/cancel", post(cancel_task))
        .route("/pent/runs", get(list_runs).post(create_run))
        .route("/pent/runs/{id}", get(get_run))
        .route("/pent/runs/{id}/cancel", post(cancel_run))
        .route("/pent/runs/{id}/events", get(get_run_events))
        .route("/pent/runs/{id}/tool-calls", get(get_tool_calls))
        .route("/pent/approvals", get(list_approvals))
        .route("/pent/approvals/{id}/decision", post(decide_approval))
        // /pent/chat removed — was a demo stub with keyword-fixed replies
        .merge(tooling::http::routes())
        .route("/pent/metrics", get(metrics))
        .route("/pent/tools/overview", get(tools_overview))
        .route("/pent/traffic", get(traffic_list))
        .route("/pent/rmos/memories", get(rmos_memories))
        .route("/pent/rmos/memories/{id}/feedback", post(rmos_feedback))
        .route("/pent/rmos/attack-graph", get(attack_graph))
        .route(
            "/pent/settings/model-role-bindings",
            get(get_model_bindings).put(update_model_bindings),
        )
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
        .route("/pent/traffic/{id}", get(get_traffic))
        .route("/pent/artifacts/{id}/download", get(download_artifact))
        .merge(session::routes())
        .merge(conversation::http::routes())
        .merge(skills::routes())
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
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM pent_projects t WHERE tenant_id=$1 AND ($2 OR created_by=$3) AND deleted=0 ORDER BY created_at DESC LIMIT 100")
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
        "SELECT to_jsonb(t) FROM pent_projects t
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

async fn list_tasks(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tasks t WHERE tenant_id=$1 AND ($2 OR created_by=$3) AND deleted=0 ORDER BY created_at DESC LIMIT 100",
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
    if body.get("authorization_confirmed").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::bad_request("必须明确确认已获得目标测试授权"));
    }
    let id = Uuid::new_v4().to_string();
    let name = body["name"].as_str().unwrap_or("untitled");
    let desc = body.get("description").and_then(|v| v.as_str());
    let target = normalize_confirmed_scope(
        body.get("target_scope").cloned().unwrap_or_default(),
        &user.user_id,
    )?;
    let priority = body["priority"].as_str().unwrap_or("medium");
    let pid = body["project_id"].as_str();
    if let Some(project_id) = pid {
        ensure_project_access(&s.pool, project_id, &user).await?;
    }
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

async fn list_runs(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object(
            'id', r.id, 'task_id', r.task_id, 'task_name', tk.name,
            'status', r.status, 'session_id', r.session_id,
            'objective_snapshot', r.objective_snapshot,
            'scope_snapshot', r.scope_snapshot,
            'started_at', r.started_at, 'completed_at', r.completed_at,
            'created_at', r.created_at, 'updated_at', r.updated_at
         )
         FROM pent_runs r
         LEFT JOIN pent_tasks tk ON tk.id = r.task_id
         WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3)
         ORDER BY r.created_at DESC LIMIT 100",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
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
            'status', r.status, 'session_id', r.session_id,
            'objective_snapshot', r.objective_snapshot,
            'scope_snapshot', r.scope_snapshot,
            'started_at', r.started_at, 'completed_at', r.completed_at,
            'created_at', r.created_at, 'updated_at', r.updated_at
         )
         FROM pent_runs r
         LEFT JOIN pent_tasks tk ON tk.id = r.task_id
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
        let payload = serde_json::to_vec(&json!({"run_id": id, "task_id": task_id})).unwrap_or_default();
        nats.publish("penta.run.requested", &payload).await;
    }
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"Run 创建成功"}),
    )))
}
async fn cancel_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query(
        "UPDATE pent_runs
         SET cancel_requested_at=now(),
             status=CASE WHEN status='running' THEN 'cancelling' ELSE 'cancelled' END,
             completed_at=CASE WHEN status='running' THEN completed_at ELSE now() END,
             updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR user_id=$4)
           AND status IN ('queued','running','waiting_approval')",
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
        "UPDATE pent_approvals SET status='cancelled',updated_at=now()
         WHERE run_id=$1 AND status='pending'",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"Run 已取消"}))))
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
async fn list_approvals(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(a) FROM pent_approvals a JOIN pent_runs r ON r.id=a.run_id WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3) AND a.status='pending' AND a.expires_at>now() ORDER BY a.created_at DESC LIMIT 50")
        .bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
        .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn decide_approval(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let decision = body["decision"].as_str().unwrap_or("denied");
    let status = if decision == "approved" {
        "approved"
    } else {
        "denied"
    };
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let run_id = sqlx::query_scalar::<_, String>(
        "UPDATE pent_approvals a
         SET status=$2,decision=$3,decided_by=$4,updated_at=now()
         FROM pent_runs r
         WHERE a.id=$1 AND r.id=a.run_id AND r.tenant_id=$5
           AND ($6 OR r.user_id=$4)
           AND a.status='pending' AND a.expires_at>now()
         RETURNING a.run_id",
    )
    .bind(&id)
    .bind(status)
    .bind(decision)
    .bind(&user.user_id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::bad_request("审批不存在、已处理或已过期"))?;
    if status == "approved" {
        sqlx::query(
            "UPDATE pent_tool_calls
             SET permission_decision='approved'
             WHERE id=(SELECT tool_call_id FROM pent_approvals WHERE id=$1)",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query(
            "UPDATE pent_runs
             SET status='queued',
                 execution_cursor=execution_cursor || jsonb_build_object(
                     'state','approved','approved_at',now()
                 ),
                 updated_at=now()
             WHERE id=$1 AND tenant_id=$2 AND status='waiting_approval'",
        )
        .bind(&run_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    } else {
        sqlx::query(
            "UPDATE pent_tool_calls
             SET permission_decision='denied'
             WHERE id=(SELECT tool_call_id FROM pent_approvals WHERE id=$1)",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query(
            "UPDATE pent_runs
             SET status='blocked',failure_code='approval_denied',
                 failure_message='中高风险验证审批被拒绝',
                 execution_cursor=execution_cursor || jsonb_build_object(
                     'state','denied','denied_at',now()
                 ),
                 completed_at=now(),updated_at=now()
             WHERE id=$1 AND tenant_id=$2 AND status='waiting_approval'",
        )
        .bind(&run_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"审批完成"}))))
}
async fn list_tools(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tool_definitions t WHERE lifecycle_status='active' ORDER BY namespace, name LIMIT 100"
    ).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

// ── Worker executor with AI agent and lease renewal ─────────────────────

/// Background worker: picks up queued runs, calls AI for analysis, manages lease.
pub async fn start_worker(pool: PgPool, worker_id: String, lease_ttl: i64) {
    if let Err(error) = register_worker(&pool, &worker_id).await {
        tracing::warn!(worker=%worker_id, error=%error, "worker registration failed");
    }
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
    sqlx::query(
        "INSERT INTO pent_workers(worker_id,capacity,status,last_heartbeat,registered_at)
         VALUES($1,1,'active',now(),now())
         ON CONFLICT(worker_id) DO UPDATE SET
             capacity=excluded.capacity,status='active',last_heartbeat=now()",
    )
    .bind(worker_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
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

// ── Metrics ─────────────────────────────────────────────────────────────

async fn metrics(State(s): State<PentState>, user: CurrentUser) -> Result<Json<Value>, AppError> {
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
    Ok(Json(json!({
        "runs": {"queued":queued,"running":running,"completed":completed,"failed":failed,"cancelled":cancelled},
        "approvals_pending": waiting,
        "sessions": sessions.unwrap_or(0),
        "findings": findings,
        "tools": tools,
        "tool_calls": calls,
        "worker_count": 1,
        "success_rate": if completed + failed > 0 { format!("{:.0}%", completed as f64 / (completed + failed) as f64 * 100.0) } else { "N/A".to_string() }
    })))
}

async fn tools_overview(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(
        json!({"total_tools":total,"total_calls":total_calls,"success_rate": format!("{:.0}%", success_rate),"healthy_tools":total,"top_tools":top}),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolAuthorization {
    Allowed,
    WaitingForApproval,
}

// Approval machinery is kept for the approvals API and its integration test;
// the unified conversation runtime enforces scope guards instead of per-tool approvals.
#[allow(dead_code)]
async fn authorize_tool_call(
    pool: &PgPool,
    run_id: &str,
    tool_name: &str,
    target: &str,
) -> Result<ToolAuthorization, String> {
    if std::env::var("PENTA_ENV").as_deref() == Ok("development")
        && std::env::var("PENTA_AUTO_APPROVE_ALL_TOOLS").as_deref() == Ok("true")
    {
        append_run_event(
            pool,
            run_id,
            "approval.auto_granted",
            json!({
                "tool_name": tool_name,
                "target": target,
                "reason": "explicit local debug override"
            }),
        )
        .await?;
        return Ok(ToolAuthorization::Allowed);
    }
    sqlx::query(
        "UPDATE pent_approvals
         SET status='expired',updated_at=now()
         WHERE run_id=$1 AND status='pending' AND expires_at<=now()",
    )
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    let definition: Option<(String, String)> = sqlx::query_as(
        "SELECT risk_level, approval_policy
         FROM pent_tool_definitions
         WHERE tool_id=$1 AND lifecycle_status='active'
         ORDER BY revision DESC
         LIMIT 1",
    )
    .bind(tool_name)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let (risk_level, approval_policy) = definition.unwrap_or_else(|| {
        let risk = match tool_name {
            "ssh_exec"
            | "sql_query"
            | "command_injection_validate"
            | "file_inclusion_validate"
            | "file_upload_validate" => "high",
            "nuclei" | "afrog" | "sqli_validate" | "xss_validate" | "csrf_validate" => "medium",
            _ => "low",
        };
        (risk.to_string(), "conditional".to_string())
    });
    let tenant_id: i64 = sqlx::query_scalar("SELECT tenant_id FROM pent_runs WHERE id=$1")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    let policy = runtime_policy::load(pool, tenant_id)
        .await
        .map_err(|error| format!("{error:?}"))?;
    // VulnClaw-style default: scope confirmation + tool-layer constraints are
    // the authorization boundary; per-tool human approval is opt-in via
    // PENTA_APPROVAL_MODE=policy. Every auto-grant is audited.
    if std::env::var("PENTA_APPROVAL_MODE").as_deref() != Ok("policy") {
        append_run_event(
            pool,
            run_id,
            "approval.auto_granted",
            json!({
                "tool_name": tool_name,
                "target": target,
                "risk_level": risk_level,
                "reason": "approval_mode=auto（默认免审批；设置 PENTA_APPROVAL_MODE=policy 恢复人工审批）"
            }),
        )
        .await?;
        return Ok(ToolAuthorization::Allowed);
    }
    let needs_approval = approval_policy == "always"
        || policy
            .require_approval_for
            .as_array()
            .is_some_and(|levels| levels.iter().any(|level| level == &risk_level))
        || (!policy.auto_approve_low_risk && risk_level == "low");
    if !needs_approval {
        return Ok(ToolAuthorization::Allowed);
    }

    let existing: Option<String> = sqlx::query_scalar(
        "SELECT a.status
         FROM pent_approvals a
         JOIN pent_tool_calls c ON c.id=a.tool_call_id
         WHERE a.run_id=$1 AND a.tool_name=$2
           AND c.params_summary->>'target'=$3
         ORDER BY a.created_at DESC
         LIMIT 1",
    )
    .bind(run_id)
    .bind(tool_name)
    .bind(target)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    match existing.as_deref() {
        Some("approved") => return Ok(ToolAuthorization::Allowed),
        Some("pending") => return Ok(ToolAuthorization::WaitingForApproval),
        Some("denied") => return Err(format!("工具 {tool_name} 的审批已拒绝")),
        _ => {}
    }

    let tool_call_id = Uuid::new_v4().to_string();
    let approval_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO pent_tool_calls(
             id,run_id,tool_name,params_summary,permission_decision
         ) VALUES($1,$2,$3,$4,'pending_approval')",
    )
    .bind(&tool_call_id)
    .bind(run_id)
    .bind(tool_name)
    .bind(json!({"target": target}))
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO pent_approvals(
             id,run_id,tool_call_id,risk_level,tool_name,description,status,
             requested_by,expires_at
         )
         SELECT $1,$2,$3,$4,$5,$6,'pending',user_id,
                now()+($7||' seconds')::interval
         FROM pent_runs WHERE id=$2",
    )
    .bind(&approval_id)
    .bind(run_id)
    .bind(&tool_call_id)
    .bind(&risk_level)
    .bind(tool_name)
    .bind(format!("在已确认目标 {target} 上执行 {tool_name}"))
    .bind(policy.approval_ttl_seconds)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE pent_runs
         SET status='waiting_approval',lease_expires_at=NULL,worker_id=NULL,
             execution_cursor=jsonb_build_object(
                 'state','waiting_approval',
                 'tool_name',$2,
                 'target',$3,
                 'tool_call_id',$4,
                 'approval_id',$5
             ),
             updated_at=now()
         WHERE id=$1 AND status='running'",
    )
    .bind(run_id)
    .bind(tool_name)
    .bind(target)
    .bind(&tool_call_id)
    .bind(&approval_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(ToolAuthorization::WaitingForApproval)
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
    Err(format!("Run {run_id} 未能关联 Agent 会话，无法进入会话运行时"))
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

// ── LLM Traffic ────────────────────────────────────────────────────────────

async fn traffic_list(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(c) FROM pent_model_calls c JOIN pent_agent_sessions s ON s.id=c.session_id WHERE s.tenant_id=$1 AND ($2 OR s.created_by=$3) ORDER BY c.started_at DESC LIMIT 200"
    ).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

// ── RMOS Memory ────────────────────────────────────────────────────────────

async fn rmos_memories(
    State(s): State<PentState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_knowledge_documents t ORDER BY created_at DESC LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn rmos_feedback(
    State(s): State<PentState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let feedback = body["feedback"].as_str().unwrap_or("");
    let tags = if feedback == "useful" {
        "[\"useful\"]"
    } else {
        "[\"useless\"]"
    };
    sqlx::query(&format!(
        "UPDATE pent_knowledge_documents SET tags = tags || '{tags}'::jsonb WHERE id = $1::uuid"
    ))
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"status":"ok","feedback":feedback}),
    )))
}

// ── Attack Graph ───────────────────────────────────────────────────────────

async fn attack_graph(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let findings: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT f.id, f.title, f.severity, f.target FROM pent_findings f JOIN pent_runs r ON r.id=f.run_id WHERE r.tenant_id=$1 AND ($2 OR r.user_id=$3) ORDER BY f.created_at DESC LIMIT 100"
    ).bind(tenant_id).bind(security::tenant_wide(&user)).bind(&user.user_id)
    .fetch_all(&s.pool).await.unwrap_or_default();
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut seen_targets = std::collections::HashSet::new();
    for (id, title, sev, target) in &findings {
        let category = match sev.as_str() {
            "critical" => 0,
            "high" => 1,
            _ => 2,
        };
        let size = match sev.as_str() {
            "critical" => 35,
            "high" => 28,
            _ => 22,
        };
        nodes.push(json!({"id":id,"name":title,"category":category,"symbolSize":size}));
        if !seen_targets.contains(target) {
            seen_targets.insert(target.clone());
            nodes.push(json!({"id":target,"name":target,"category":3,"symbolSize":45,"symbol":"roundRect"}));
        }
        edges.push(json!({"source":target,"target":id}));
    }
    Ok(Json(ApiResponse::new(json!({"nodes":nodes,"edges":edges}))))
}

// ── Platform Settings ──────────────────────────────────────────────────────

async fn get_model_bindings(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_model_role_bindings t WHERE tenant_id=$1 AND enabled=true ORDER BY model_role"
    ).bind(tenant_id).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn update_model_bindings(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    if let Some(bindings) = body["bindings"].as_array() {
        for b in bindings {
            sqlx::query("UPDATE pent_model_role_bindings SET model_config_id=$4,temperature=$5,max_output_tokens=$6,timeout_seconds=$7,enabled=$8,updated_at=now() WHERE tenant_id=$1 AND model_role=$2 AND project_id=$3")
                .bind(tenant_id).bind(b["model_role"].as_str().unwrap_or("")).bind(b["project_id"].as_str().unwrap_or(""))
                .bind(b["model_config_id"].as_i64().unwrap_or(0)).bind(b["temperature"].as_f64().unwrap_or(0.7))
                .bind(b["max_output_tokens"].as_i64().unwrap_or(4096) as i32).bind(b["timeout_seconds"].as_i64().unwrap_or(120) as i32)
                .bind(b["enabled"].as_bool().unwrap_or(true))
                .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
        }
    }
    Ok(Json(ApiResponse::new(json!({"message":"已保存"}))))
}

async fn get_runtime_policy(
    State(s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let policy = runtime_policy::load(&s.pool, tenant_id).await?;
    Ok(Json(ApiResponse::new(json!(policy))))
}

async fn update_runtime_policy(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<runtime_policy::UpdateRuntimePolicy>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let policy = runtime_policy::save(&s.pool, tenant_id, &user.user_id, body).await?;
    Ok(Json(ApiResponse::new(json!(policy))))
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
                obj.insert(
                    field.into(),
                    Value::String(sanitize_sensitive_data(text)),
                );
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

#[cfg(test)]
mod approval_mode_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires local PostgreSQL (TEST_DATABASE_URL)"]
    async fn auto_approval_mode_allows_high_risk_tool_without_human_decision() {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        let pool = kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database");
        let run_id = "approval-it-run";

        sqlx::query("DELETE FROM pent_run_events WHERE run_id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup events");
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup run");
        sqlx::query("DELETE FROM pent_tool_definitions WHERE tool_id='it-approval-tool'")
            .execute(&pool)
            .await
            .expect("cleanup tool");
        sqlx::query(
            "INSERT INTO pent_runs(id, task_id, tenant_id, user_id, status, objective_snapshot)
             VALUES ($1,'approval-it-task',0,'it-tester','running','审批模式测试')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed run");
        // A tool whose manifest demands approval on every call.
        sqlx::query(
            "INSERT INTO pent_tool_definitions(
                 tool_id, version, namespace, name, description, source,
                 risk_level, approval_policy, lifecycle_status
             ) VALUES ('it-approval-tool','1.0.0','test','审批测试工具','测试','builtin',
                       'high','always','active')",
        )
        .execute(&pool)
        .await
        .expect("seed tool");

        // Default mode (no PENTA_APPROVAL_MODE=policy): auto-grant.
        let result = authorize_tool_call(&pool, run_id, "it-approval-tool", "192.0.2.10").await;
        assert!(
            matches!(result, Ok(ToolAuthorization::Allowed)),
            "默认模式必须自动批准，实际：{result:?}"
        );
        let audited: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pent_run_events
             WHERE run_id=$1 AND event_type='approval.auto_granted')",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("query audit event");
        assert!(audited, "自动批准必须留下审计事件");
        let run_status: String = sqlx::query_scalar(
            "SELECT status FROM pent_runs WHERE id=$1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("query run status");
        assert_eq!(run_status, "running", "免审批不得改变 Run 状态");

        sqlx::query("DELETE FROM pent_run_events WHERE run_id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup events after test");
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup run after test");
        sqlx::query("DELETE FROM pent_tool_definitions WHERE tool_id='it-approval-tool'")
            .execute(&pool)
            .await
            .expect("cleanup tool after test");
    }
}
