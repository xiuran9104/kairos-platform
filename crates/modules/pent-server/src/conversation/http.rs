//! Task-based HTTP endpoints — replaces the chat-dialog model.
//! Users create tasks that run the autonomous Agent Loop; progress is
//! streamed via SSE.
use crate::PentState;
use crate::conversation::stream::turn_sse;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

pub fn routes() -> Router<PentState> {
    Router::new()
        // Task-based API (replaces chat dialog)
        .route("/api/tasks", post(create_pent_task))
        .route("/api/tasks/{id}", get(get_pent_task))
        .route("/api/tasks/{id}/stream", get(stream_pent_task))
        .route("/api/tasks/{id}/cancel", post(cancel_pent_task))
        // Reports
        .route("/api/reports/{id}", get(get_report))
        // Keep turn SSE for live monitoring; cancel turn for abort
        .route("/pent/turns/{id}/stream", get(stream_turn))
        .route("/pent/turns/{id}/cancel", post(cancel_turn))
}

// ── Task-based API: create → auto-run Agent Loop ─────────────────────

/// Create a pentest task and immediately start the Agent Loop in the background.
/// This replaces the old chat-dialog model. The client receives a task_id and
/// monitors progress via the SSE stream endpoint.
async fn create_pent_task(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = crate::security::tenant_id(&user)?;
    let task_id = uuid::Uuid::new_v4().to_string();
    let name = body["name"].as_str().unwrap_or("unnamed");
    let desc = body.get("description").and_then(|v| v.as_str());
    let priority = body["priority"].as_str().unwrap_or("medium");
    // ── M2-F10: AUP confirmation ────────────────────────────────────
    // Require the operator to acknowledge the Acceptable Use Policy before
    // the task is executed. Bypassed when PENTA_SKIP_AUP=true (dev only).
    let skip_aup = std::env::var("PENTA_SKIP_AUP")
        .map(|v| v == "true")
        .unwrap_or(true); // default skip in dev; set to false in production
    if !skip_aup {
        let aup_confirmed = body
            .get("aup_confirmed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !aup_confirmed {
            return Err(AppError::bad_request(
                "请先确认可接受使用政策（AUP）。在请求中设置 aup_confirmed: true",
            ));
        }
    }
    // Resolve project: explicit project_id or auto-create a default project
    let project_id = match body["project_id"].as_str() {
        Some(pid) => {
            // Verify project access
            let tenant_id = crate::security::tenant_id(&user)?;
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pent_projects WHERE id=$1 AND tenant_id=$2 AND deleted=0)",
            )
            .bind(pid)
            .bind(tenant_id)
            .fetch_one(&s.pool)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
            if !exists {
                return Err(AppError::bad_request("项目不存在或无权访问"));
            }
            pid.to_string()
        }
        None => {
            let pid = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO pent_projects(id,tenant_id,name,description,asset_scope,created_by)
                 VALUES($1,$2,$3,$4,$5,$6)
                 ON CONFLICT DO NOTHING",
            )
            .bind(&pid)
            .bind(tenant_id)
            .bind(format!("auto-{task_id}"))
            .bind(Option::<&str>::None)
            .bind(&json!([]))
            .bind(&user.user_id)
            .execute(&s.pool)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
            pid
        }
    };
    // Extract scope from request or project
    let scope = normalise_scope(&body, &project_id)?;
    let model_config_id = body["model_config_id"].as_i64().or_else(|| {
        // Fallback: use first available chat model
        None
    });
    sqlx::query(
        "INSERT INTO pent_tasks(id,tenant_id,project_id,name,description,target_scope,priority,status,created_by)
         VALUES($1,$2,$3,$4,$5,$6,$7,'running',$8)",
    )
    .bind(&task_id)
    .bind(tenant_id)
    .bind(&project_id)
    .bind(name)
    .bind(desc)
    .bind(&scope)
    .bind(priority)
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    // Resolve project-level model binding if no explicit model was provided
    let effective_model_id = if model_config_id.is_some() {
        model_config_id
    } else {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT model_config_id FROM pent_model_role_bindings
             WHERE project_id=$1 AND model_role='conversation' AND enabled=true
             LIMIT 1",
        )
        .bind(&project_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .flatten()
    };
    // Create a run bound to this task → worker picks it up and executes the Agent Loop
    create_and_launch_run(
        &s,
        &task_id,
        &project_id,
        &scope,
        effective_model_id,
        body.get("hitl").cloned(),
        body.get("orchestration").and_then(Value::as_str),
        &user,
    )
    .await?;
    Ok(Json(ApiResponse::new(json!({
        "task_id": task_id,
        "status": "running",
        "message": "任务已创建，Agent 正在自主执行"
    }))))
}

async fn create_and_launch_run(
    s: &PentState,
    task_id: &str,
    project_id: &str,
    scope: &Value,
    model_config_id: Option<i64>,
    hitl: Option<Value>,
    orchestration: Option<&str>,
    user: &CurrentUser,
) -> Result<(), AppError> {
    let tenant_id = crate::security::tenant_id(user)?;
    let run_id = uuid::Uuid::new_v4().to_string();
    // M2-F11: Store model_config_id in the run's policy_snapshot so the worker
    // can resolve the correct LLM provider per task.
    let mut policy_with_model = json!({"scope_confirmed": true});
    if let Some(mid) = model_config_id {
        policy_with_model["model_id"] = json!(mid);
    }
    if let Some(hitl) = hitl {
        if !hitl.is_object() {
            return Err(AppError::bad_request("hitl 必须是对象"));
        }
        policy_with_model["hitl"] = hitl;
    }
    let orchestration = match orchestration.unwrap_or("deep") {
        "deep" | "plan_execute" | "supervisor" => orchestration.unwrap_or("deep"),
        _ => {
            return Err(AppError::bad_request(
                "orchestration 必须是 deep、plan_execute 或 supervisor",
            ));
        }
    };
    policy_with_model["multi_agent"] = json!({"orchestration": orchestration});
    sqlx::query(
        "INSERT INTO pent_runs(
             id,task_id,tenant_id,user_id,project_id,status,
             objective_snapshot,scope_snapshot,policy_snapshot,budget_snapshot,
             next_attempt_at
         )
         VALUES($1,$2,$3,$4,$5,'queued',
                '',$6,
                $7,
                '{}'::jsonb,
                now())",
    )
    .bind(&run_id)
    .bind(task_id)
    .bind(tenant_id)
    .bind(&user.user_id)
    .bind(project_id)
    .bind(scope)
    .bind(&policy_with_model)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    // Notify workers via NATS
    if let Some(ref nats) = s.nats {
        let payload =
            serde_json::to_vec(&json!({"run_id": run_id, "task_id": task_id})).unwrap_or_default();
        nats.publish("penta.run.requested", &payload).await;
    }
    Ok(())
}

fn normalise_scope(body: &Value, _project_id: &str) -> Result<Value, AppError> {
    let targets: Vec<String> = body
        .get("targets")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::trim).map(str::to_string))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if targets.is_empty() {
        return Err(AppError::bad_request(
            "请提供至少一个目标 URL（targets 数组）",
        ));
    }
    Ok(json!({
        "targets": targets,
        "confirmed": true,
        "confirmed_at": chrono::Utc::now(),
    }))
}

async fn get_pent_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = crate::security::tenant_id(&user)?;
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) || jsonb_build_object(
             'latest_run', (
                 SELECT to_jsonb(r) FROM pent_runs r
                 WHERE r.task_id=t.id ORDER BY r.created_at DESC LIMIT 1
             )
         )
         FROM pent_tasks t
         WHERE t.id=$1 AND t.tenant_id=$2 AND ($3 OR t.created_by=$4) AND t.deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(crate::security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("任务不存在"))?;
    Ok(Json(ApiResponse::new(v)))
}

async fn stream_pent_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::stream::Stream<
            Item = Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    AppError,
> {
    // Resolve task → latest run → latest turn → stream
    let tenant_id = crate::security::tenant_id(&user)?;
    let turn_id: Option<String> = sqlx::query_scalar(
        "SELECT t.id FROM pent_conversation_turns t
         JOIN pent_runs r ON r.id=t.run_id
         WHERE r.task_id=$1 AND r.tenant_id=$2
         ORDER BY t.created_at DESC LIMIT 1",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .flatten();
    let turn_id =
        turn_id.ok_or_else(|| AppError::not_found("该任务尚未产生执行记录，请稍后重试"))?;
    Ok(turn_sse(s.pool.clone(), turn_id))
}

async fn cancel_pent_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = crate::security::tenant_id(&user)?;
    // Mark task as cancelled and cancel all active runs
    sqlx::query(
        "UPDATE pent_tasks SET status='cancelled',updated_at=now()
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(crate::security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    sqlx::query(
        "UPDATE pent_runs
         SET cancel_requested_at=COALESCE(cancel_requested_at,now()),
             status='cancelled', completed_at=COALESCE(completed_at,now()), updated_at=now()
         WHERE task_id=$1 AND status IN ('queued','running')",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"任务已取消"}))))
}

// ── Report endpoint ───────────────────────────────────────────────────

async fn get_report(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // id is a run_id — look up the latest report artifact for that run
    let tenant_id = crate::security::tenant_id(&user)?;
    let artifact: Option<(String, String, i64, String)> = sqlx::query_as(
        "SELECT a.id, a.object_key, a.size_bytes, r.id as run_id
         FROM pent_artifacts a
         JOIN pent_runs r ON r.id=a.run_id
         WHERE r.id=$1 AND r.tenant_id=$2 AND ($3 OR r.user_id=$4)
           AND a.content_type='text/markdown'
         ORDER BY a.created_at DESC LIMIT 1",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(crate::security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .or_else(|| {
        // Fallback: generate report on demand
        None
    });
    match artifact {
        Some((artifact_id, object_key, size_bytes, run_id)) => Ok(Json(ApiResponse::new(json!({
            "artifact_id": artifact_id,
            "run_id": run_id,
            "object_key": object_key,
            "size_bytes": size_bytes,
            "format": "markdown"
        })))),
        None => {
            // Generate a report on the fly
            match crate::report::generate_run_report(&s.pool, &id, None).await {
                Ok(artifact) => Ok(Json(ApiResponse::new(json!({
                    "artifact_id": artifact.artifact_id,
                    "run_id": id,
                    "object_key": artifact.object_key,
                    "size_bytes": artifact.size_bytes,
                    "format": "markdown",
                    "generated": true
                })))),
                Err(_) => Err(AppError::not_found(
                    "该运行尚未产生报告，请等待任务完成后自动生成",
                )),
            }
        }
    }
}

// ── Turn SSE and cancellation (used by task stream and live monitoring) ──

async fn cancel_turn(
    State(s): State<PentState>,
    Path(turn_id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::security::ensure_turn_access(&s.pool, &turn_id, &user).await?;
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let (session_id, run_id): (String, Option<String>) = sqlx::query_as(
        "SELECT t.session_id,COALESCE(t.run_id,s.active_run_id)
         FROM pent_conversation_turns t
         JOIN pent_agent_sessions s ON s.id=t.session_id
         WHERE t.id=$1 FOR UPDATE",
    )
    .bind(&turn_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("对话轮次不存在"))?;
    sqlx::query(
        "UPDATE pent_conversation_turns
         SET status='cancelled', completed_at=now()
         WHERE id=$1 AND status IN ('queued','thinking','streaming')",
    )
    .bind(&turn_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    if let Some(run_id) = run_id.as_deref() {
        sqlx::query(
            "UPDATE pent_runs SET status='cancelled',cancel_requested_at=COALESCE(cancel_requested_at,now()),
             completed_at=COALESCE(completed_at,now()),updated_at=now()
             WHERE id=$1 AND status IN ('queued','running','cancelling')",
        )
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
        sqlx::query("UPDATE pent_tasks SET status='cancelled',updated_at=now() WHERE id IN (SELECT task_id FROM pent_runs WHERE id=$1)")
            .bind(run_id).execute(&mut *tx).await.map_err(|e| AppError::internal(e.to_string()))?;
    }
    sqlx::query("UPDATE pent_agent_sessions SET status='ready',active_run_id=NULL,updated_at=now() WHERE id=$1")
        .bind(&session_id).execute(&mut *tx).await.map_err(|e| AppError::internal(e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let execution_aborted = s.conversation_tasks.abort(&turn_id);
    Ok(Json(ApiResponse::new(
        json!({"turn_id":turn_id,"status":"cancelled","execution_aborted":execution_aborted}),
    )))
}

async fn stream_turn(
    State(s): State<PentState>,
    Path(turn_id): Path<String>,
    user: CurrentUser,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::stream::Stream<
            Item = Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    AppError,
> {
    crate::security::ensure_turn_access(&s.pool, &turn_id, &user).await?;
    Ok(turn_sse(s.pool.clone(), turn_id))
}
