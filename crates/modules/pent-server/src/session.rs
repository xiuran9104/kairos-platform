use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_pent_api::{
    ConfirmScopeRequest, CreateAgentSessionRequest, CreateConversationMessageRequest,
    CreateRunInstructionRequest, CreateSessionRunRequest,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{ApiResponse, PentState, security};

pub(crate) fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/sessions", get(list_sessions).post(create_session))
        .route(
            "/pent/sessions/{id}",
            get(get_session).delete(delete_session),
        )
        .route("/pent/sessions/{id}/model", axum::routing::put(update_session_model))
        .route(
            "/pent/sessions/{id}/messages",
            get(list_messages).post(create_message),
        )
        .route("/pent/sessions/{id}/scope/confirm", post(confirm_scope))
        .route("/pent/sessions/{id}/runs", post(create_session_run))
        .route("/pent/runs/{id}/instructions", post(create_instruction))
}

async fn list_sessions(
    State(state): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(s) || jsonb_build_object(
             'active_run_id',
             CASE WHEN EXISTS(
                 SELECT 1 FROM pent_runs r
                 WHERE r.id=s.active_run_id AND r.status IN ('queued','running')
             ) THEN s.active_run_id ELSE NULL END,
             'run_status', COALESCE(
                 (SELECT r.status FROM pent_runs r
                  WHERE r.session_id=s.id ORDER BY r.created_at DESC LIMIT 1),
                 s.status
             )
         )
         FROM pent_agent_sessions s
         WHERE tenant_id=$1 AND ($2 OR created_by=$3)
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn get_session(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let session = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(s) || jsonb_build_object(
             'active_run_id',
             CASE WHEN EXISTS(
                 SELECT 1 FROM pent_runs r
                 WHERE r.id=s.active_run_id AND r.status IN ('queued','running')
             ) THEN s.active_run_id ELSE NULL END,
             'run_status', COALESCE(
                 (SELECT r.status FROM pent_runs r
                  WHERE r.session_id=s.id ORDER BY r.created_at DESC LIMIT 1),
                 s.status
             )
         ) FROM pent_agent_sessions s
         WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or_else(|| AppError::not_found("Agent Session 不存在"))?;
    Ok(Json(ApiResponse::new(session)))
}

async fn delete_session(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    security::ensure_session_access(&state.pool, &id, &user).await?;
    let mut tx = state.pool.begin().await.map_err(internal)?;
    sqlx::query("DELETE FROM pent_conversation_messages WHERE session_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    sqlx::query("DELETE FROM pent_run_instructions WHERE session_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    sqlx::query("DELETE FROM pent_runs WHERE session_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    let affected = sqlx::query("DELETE FROM pent_agent_sessions WHERE id = $1 AND tenant_id=$2")
        .bind(&id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .rows_affected();
    tx.commit().await.map_err(internal)?;
    if affected == 0 {
        return Err(AppError::not_found("Session 不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted": id}))))
}

async fn update_session_model(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(request): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::security::ensure_session_access(&state.pool, &id, &user).await?;
    let model_id = request
        .get("model_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::bad_request("请选择模型"))?;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ai.model_configs WHERE id=$1 AND type='chat' AND status=0)",
    )
    .bind(model_id)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;
    if !valid {
        return Err(AppError::bad_request("模型不存在、未启用或不是聊天模型"));
    }
    let affected = sqlx::query(
        "UPDATE pent_agent_sessions
         SET constraints=jsonb_set(constraints,'{model_id}',to_jsonb($2::bigint),true),updated_at=now()
         WHERE id=$1 AND active_run_id IS NULL",
    )
    .bind(&id)
    .bind(model_id)
    .execute(&state.pool)
    .await
    .map_err(internal)?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("会话运行中，暂时不能切换模型"));
    }
    Ok(Json(ApiResponse::new(json!({"model_id": model_id}))))
}

async fn create_session(
    State(state): State<PentState>,
    user: CurrentUser,
    Json(request): Json<CreateAgentSessionRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let objective = request.objective.trim();
    if objective.is_empty() {
        return Err(AppError::bad_request("测试目标不能为空"));
    }
    if request.targets.iter().any(|t| t.trim().is_empty()) {
        return Err(AppError::bad_request("目标范围不能包含空值"));
    }

    let session_id = Uuid::new_v4().to_string();
    let confirmed_at = chrono::Utc::now();
    let proposal = json!({
        "targets": request.targets,
        "constraints": request.constraints,
        "source": "user_input",
        "confirmed": true,
        "confirmed_at": confirmed_at,
        "confirmed_by": user.user_id
    });
    let message_id = Uuid::new_v4().to_string();
    let assistant_message_id = Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await.map_err(internal)?;
    if let Some(project_id) = request.project_id.as_deref() {
        let project_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM pent_projects
                 WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0
             )",
        )
        .bind(project_id)
        .bind(tenant_id)
        .bind(security::tenant_wide(&user))
        .bind(&user.user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
        if !project_exists {
            return Err(AppError::bad_request("项目不存在或无权访问"));
        }
    }

    sqlx::query(
        "INSERT INTO pent_agent_sessions(
             id, tenant_id, project_id, objective, constraints, budget,
             scope_proposal, confirmed_scope_snapshot, status, created_by
         ) VALUES($1, $2, $3, $4, $5, $6, $7, $7, 'ready', $8)",
    )
    .bind(&session_id)
    .bind(tenant_id)
    .bind(request.project_id)
    .bind(objective)
    .bind(&request.constraints)
    .bind(&request.budget)
    .bind(&proposal)
    .bind(&user.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    sqlx::query(
        "INSERT INTO pent_conversation_messages(
             id, session_id, role, message_type, content, payload, created_by
         ) VALUES($1, $2, 'user', 'objective', $3, $4, $5)",
    )
    .bind(message_id)
    .bind(&session_id)
    .bind(objective)
    .bind(json!({"scope_proposal": proposal}))
    .bind(&user.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    sqlx::query(
        "INSERT INTO pent_conversation_messages(
             id, session_id, role, message_type, content, payload, created_by
         ) VALUES($1, $2, 'assistant', 'scope_proposal', $3, $4, 'system')",
    )
    .bind(assistant_message_id)
    .bind(&session_id)
    .bind("请输入渗透测试目标。")
    .bind(json!({"scope_proposal": proposal}))
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    tx.commit().await.map_err(internal)?;
    Ok(Json(ApiResponse::new(json!({
        "id": session_id,
        "status": "ready",
        "scope_proposal": proposal
    }))))
}

async fn list_messages(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    security::ensure_session_access(&state.pool, &id, &user).await?;
    let messages = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(m)
         FROM pent_conversation_messages m
         WHERE session_id = $1
         ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(ApiResponse::new(messages)))
}

async fn create_message(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(request): Json<CreateConversationMessageRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::ensure_session_access(&state.pool, &id, &user).await?;
    let content = request.content.trim();
    if content.is_empty() {
        return Err(AppError::bad_request("消息内容不能为空"));
    }

    let message_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pent_conversation_messages(
             id, session_id, role, message_type, content, payload, created_by
         ) VALUES($1, $2, 'user', $3, $4, $5, $6)",
    )
    .bind(&message_id)
    .bind(&id)
    .bind(&request.message_type)
    .bind(content)
    .bind(&request.payload)
    .bind(&user.user_id)
    .execute(&state.pool)
    .await
    .map_err(internal)?;

    Ok(Json(ApiResponse::new(json!({
        "id": message_id,
        "session_id": id,
        "status": "accepted"
    }))))
}

async fn confirm_scope(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(request): Json<ConfirmScopeRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    if request.targets.is_empty() {
        return Err(AppError::bad_request("至少需要一个授权目标"));
    }
    if request
        .targets
        .iter()
        .any(|target| target.trim().is_empty())
    {
        return Err(AppError::bad_request("授权目标不能包含空值"));
    }

    let snapshot = json!({
        "targets": request.targets,
        "constraints": request.constraints,
        "confirmed": true,
        "confirmed_at": chrono::Utc::now(),
        "confirmed_by": user.user_id
    });
    let affected = sqlx::query(
        "UPDATE pent_agent_sessions
         SET confirmed_scope_snapshot = $2, status = 'ready', updated_at = now()
         WHERE id=$1 AND tenant_id=$3 AND status IN ('draft', 'ready')
           AND ($4 OR created_by=$5)",
    )
    .bind(&id)
    .bind(&snapshot)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .execute(&state.pool)
    .await
    .map_err(internal)?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request(
            "Session 不存在或当前状态不能确认范围",
        ));
    }

    Ok(Json(ApiResponse::new(json!({
        "id": id,
        "status": "ready",
        "confirmed_scope_snapshot": snapshot
    }))))
}

async fn create_session_run(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(request): Json<CreateSessionRunRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let row: Option<(Option<String>, String, Value, Value, Value)> = sqlx::query_as(
        "SELECT project_id, objective, confirmed_scope_snapshot, constraints, budget
         FROM pent_agent_sessions
         WHERE id=$1 AND tenant_id=$2 AND status='ready'
           AND ($3 OR created_by=$4)",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(security::tenant_wide(&user))
    .bind(&user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?;
    let (project_id, objective, scope, constraints, budget) =
        row.ok_or_else(|| AppError::bad_request("会话不存在或状态不可用"))?;

    let task_id = Uuid::new_v4().to_string();
    let run_id = Uuid::new_v4().to_string();
    let event_id = Uuid::new_v4().to_string();
    let name = request
        .name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| objective.chars().take(80).collect());
    let priority = request.priority.unwrap_or_else(|| "medium".to_string());
    let mut tx = state.pool.begin().await.map_err(internal)?;

    sqlx::query(
        "INSERT INTO pent_tasks(
             id, tenant_id, project_id, name, description, target_scope,
             priority, status, created_by
         ) VALUES($1, $2, $3, $4, $5, $6, $7, 'queued', $8)",
    )
    .bind(&task_id)
    .bind(tenant_id)
    .bind(&project_id)
    .bind(name)
    .bind(&objective)
    .bind(&scope)
    .bind(priority)
    .bind(&user.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    sqlx::query(
        "INSERT INTO pent_runs(
             id, task_id, tenant_id, user_id, project_id, session_id, status,
             objective_snapshot, scope_snapshot, policy_snapshot, budget_snapshot
         ) VALUES($1, $2, $3, $4, $5, $6, 'queued', $7, $8, $9, $10)",
    )
    .bind(&run_id)
    .bind(&task_id)
    .bind(tenant_id)
    .bind(&user.user_id)
    .bind(&project_id)
    .bind(&id)
    .bind(&objective)
    .bind(&scope)
    .bind(json!({"constraints": constraints}))
    .bind(&budget)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    sqlx::query(
        "INSERT INTO pent_run_events(
             event_id, run_id, sequence, event_type, payload
         ) VALUES($1, $2, 0, 'run.created', $3)",
    )
    .bind(event_id)
    .bind(&run_id)
    .bind(json!({"session_id": id, "objective": objective}))
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    sqlx::query(
        "UPDATE pent_agent_sessions
         SET active_run_id = $2, status = 'running', updated_at = now()
         WHERE id = $1",
    )
    .bind(&id)
    .bind(&run_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    tx.commit().await.map_err(internal)?;
    Ok(Json(ApiResponse::new(json!({
        "session_id": id,
        "task_id": task_id,
        "run_id": run_id,
        "status": "queued"
    }))))
}

async fn create_instruction(
    State(state): State<PentState>,
    Path(run_id): Path<String>,
    user: CurrentUser,
    Json(request): Json<CreateRunInstructionRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::ensure_run_access(&state.pool, &run_id, &user).await?;
    let allowed = [
        "steer",
        "provide_context",
        "request_review",
        "request_summary",
        "pause",
        "resume",
        "stop",
    ];
    if !allowed.contains(&request.instruction_type.as_str()) {
        return Err(AppError::bad_request("不支持的运行指令类型"));
    }
    if request.content.trim().is_empty() {
        return Err(AppError::bad_request("指令内容不能为空"));
    }

    let session_id: Option<String> =
        sqlx::query_scalar("SELECT session_id FROM pent_runs WHERE id = $1")
            .bind(&run_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?
            .flatten();
    let instruction_id = Uuid::new_v4().to_string();
    let status = if request.instruction_type == "stop" {
        "applied"
    } else {
        "pending"
    };
    let mut tx = state.pool.begin().await.map_err(internal)?;

    sqlx::query(
        "INSERT INTO pent_run_instructions(
             id, session_id, run_id, instruction_type, content, payload,
             status, created_by, applied_at
         ) VALUES($1, $2, $3, $4, $5, $6, $7, $8,
                  CASE WHEN $7 = 'applied' THEN now() ELSE NULL END)",
    )
    .bind(&instruction_id)
    .bind(&session_id)
    .bind(&run_id)
    .bind(&request.instruction_type)
    .bind(request.content.trim())
    .bind(&request.payload)
    .bind(status)
    .bind(&user.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    let sequence: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(sequence), -1) + 1
         FROM pent_run_events
         WHERE run_id = $1",
    )
    .bind(&run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    sqlx::query(
        "INSERT INTO pent_run_events(
             event_id, run_id, sequence, event_type, payload
         ) VALUES($1, $2, $3, 'run.instruction.received', $4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&run_id)
    .bind(sequence)
    .bind(json!({
        "instruction_id": instruction_id,
        "instruction_type": request.instruction_type,
        "status": status
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    if status == "applied" {
        sqlx::query(
            "UPDATE pent_runs
             SET status = 'cancelled', completed_at = now(), updated_at = now()
             WHERE id = $1 AND status NOT IN ('completed', 'failed', 'cancelled')",
        )
        .bind(&run_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
        if let Some(session_id) = &session_id {
            sqlx::query(
                "UPDATE pent_agent_sessions
                 SET status = 'cancelled', updated_at = now()
                 WHERE id = $1",
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(internal)?;
        }
    }

    tx.commit().await.map_err(internal)?;
    Ok(Json(ApiResponse::new(json!({
        "id": instruction_id,
        "run_id": run_id,
        "status": status
    }))))
}

fn internal(error: sqlx::Error) -> AppError {
    AppError::internal(error.to_string())
}
