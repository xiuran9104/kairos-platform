use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
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
            get(get_session).put(update_session).delete(delete_session),
        )
        .route("/pent/sessions/{id}/model", axum::routing::put(update_session_model))
        .route(
            "/pent/sessions/{id}/messages",
            get(list_messages).post(create_message),
        )
        .route("/pent/sessions/{id}/scope/confirm", post(confirm_scope))
        .route("/pent/sessions/{id}/runs", post(create_session_run))
        .route("/pent/sessions/{id}/role", axum::routing::put(update_session_role))
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
    let mut resolved = Vec::with_capacity(rows.len());
    for row in rows {
        resolved.push(
            crate::role_resolution::decorate_session(&state.pool, tenant_id, row)
                .await
                .map_err(|error| AppError::internal(error.to_string()))?,
        );
    }
    Ok(Json(ApiResponse::new(resolved)))
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
    let session = crate::role_resolution::decorate_session(&state.pool, tenant_id, session)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(session)))
}

async fn update_session(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    security::ensure_session_access(&state.pool, &id, &user).await?;

    let objective = body
        .get("objective")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let project_id = body
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let targets: Vec<String> = body
        .get("targets")
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|t| t.as_str().map(str::trim).map(str::to_owned))
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // 目标范围与项目权威来源同步（与 create_session 同口径）
    let (final_targets, project_id) = if let Some(project_id) = project_id {
        let scope = sqlx::query_scalar::<_, Value>(
            "SELECT asset_scope FROM pent_projects
             WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
        )
        .bind(&project_id)
        .bind(tenant_id)
        .bind(security::tenant_wide(&user))
        .bind(&user.user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::bad_request("项目不存在或无权访问"))?;
        let project_targets = extract_scope_targets(Some(&scope));
        let mut resolved = targets.clone();
        if resolved.is_empty() {
            resolved = project_targets.clone();
        }
        if project_targets.is_empty() && !resolved.is_empty() {
            sqlx::query(
                "UPDATE pent_projects SET asset_scope=$2, updated_at=now()
                 WHERE id=$1 AND tenant_id=$3 AND deleted=0",
            )
            .bind(&project_id)
            .bind(serde_json::Value::Array(
                resolved.iter().cloned().map(Value::String).collect(),
            ))
            .bind(tenant_id)
            .execute(&state.pool)
            .await
            .map_err(internal)?;
        }
        (resolved, Some(project_id))
    } else {
        (targets, None)
    };

    let mut tx = state.pool.begin().await.map_err(internal)?;
    if let Some(objective) = objective {
        sqlx::query(
            "UPDATE pent_agent_sessions SET objective=$2, updated_at=now()
             WHERE id=$1 AND tenant_id=$3",
        )
        .bind(&id)
        .bind(&objective)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    }
    if let Some(project_id) = project_id {
        sqlx::query(
            "UPDATE pent_agent_sessions SET project_id=$2, updated_at=now()
             WHERE id=$1 AND tenant_id=$3",
        )
        .bind(&id)
        .bind(&project_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    }
    if !final_targets.is_empty() {
        let snapshot = json!({
            "targets": final_targets,
            "constraints": {},
            "confirmed": true,
            "confirmed_at": chrono::Utc::now(),
            "confirmed_by": user.user_id
        });
        sqlx::query(
            "UPDATE pent_agent_sessions
             SET confirmed_scope_snapshot=$2, scope_proposal=$2, updated_at=now()
             WHERE id=$1 AND tenant_id=$3",
        )
        .bind(&id)
        .bind(&snapshot)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    }
    tx.commit().await.map_err(internal)?;

    Ok(Json(ApiResponse::new(json!({"id": id, "updated": true}))))
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
        "SELECT EXISTS(SELECT 1 FROM ai.model_configs WHERE id=$1 AND type='chat' AND status=0 AND deleted=false)",
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

async fn update_session_role(
    State(state): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(request): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::security::ensure_session_access(&state.pool, &id, &user).await?;
    let tenant_id = security::tenant_id(&user)?;
    if !request.is_object() || request.get("role_id").is_none() {
        return Err(AppError::bad_request("role_id 字段不能为空"));
    }
    let role_id = request
        .get("role_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(role_id) = role_id
        && !crate::role_resolution::ensure_visible_role(&state.pool, tenant_id, role_id)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?
    {
        return Err(AppError::bad_request("角色不存在或未启用"));
    }
    let affected = sqlx::query(
        "UPDATE pent_agent_sessions
         SET constraints=CASE
             WHEN $2::text IS NULL THEN constraints - 'role_id'
             ELSE jsonb_set(constraints, '{role_id}', to_jsonb($2::text), true)
         END, updated_at=now()
         WHERE id=$1 AND active_run_id IS NULL",
    )
    .bind(&id)
    .bind(role_id)
    .execute(&state.pool)
    .await
    .map_err(internal)?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::bad_request("会话运行中，暂时不能切换角色"));
    }
    let session = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(s) FROM pent_agent_sessions s WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;
    let session = crate::role_resolution::decorate_session(&state.pool, tenant_id, session)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(session)))
}

/// Extract the target list from a project `asset_scope` value (JSON array or
/// `{ "targets": [...] }` object), normalised to trimmed non-empty strings.
fn extract_scope_targets(scope: Option<&Value>) -> Vec<String> {
    let Some(scope) = scope else {
        return Vec::new();
    };
    let array = match scope {
        Value::Array(targets) => targets.clone(),
        Value::Object(object) => object
            .get("targets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    array
        .into_iter()
        .filter_map(|target| target.as_str().map(str::trim).map(str::to_owned))
        .filter(|target| !target.is_empty())
        .collect()
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

    // 项目是目标的权威来源：绑定项目时继承项目目标，项目未配置则回写对话目标。
    let project_scope = match request.project_id.as_deref() {
        Some(project_id) => {
            let scope = sqlx::query_scalar::<_, Value>(
                "SELECT asset_scope FROM pent_projects
                 WHERE id=$1 AND tenant_id=$2 AND ($3 OR created_by=$4) AND deleted=0",
            )
            .bind(project_id)
            .bind(tenant_id)
            .bind(security::tenant_wide(&user))
            .bind(&user.user_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?;
            match scope {
                Some(scope) => Some(scope),
                None => return Err(AppError::bad_request("项目不存在或无权访问")),
            }
        }
        None => None,
    };
    let project_targets = extract_scope_targets(project_scope.as_ref());
    let mut targets = request.targets.clone();
    if targets.is_empty() {
        targets = project_targets.clone();
    }
    if targets.is_empty() {
        return Err(AppError::bad_request(
            "请至少提供一个授权目标，或在项目设置中配置目标范围",
        ));
    }
    if let Some(project_id) = request.project_id.as_deref()
        && project_targets.is_empty()
    {
        sqlx::query(
            "UPDATE pent_projects SET asset_scope=$2, updated_at=now()
             WHERE id=$1 AND tenant_id=$3 AND deleted=0",
        )
        .bind(project_id)
        .bind(serde_json::Value::Array(
            targets.iter().cloned().map(Value::String).collect(),
        ))
        .bind(tenant_id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    }

    let session_id = Uuid::new_v4().to_string();
    let confirmed_at = chrono::Utc::now();
    let proposal = json!({
        "targets": targets,
        "constraints": request.constraints,
        "source": if project_scope.is_some() { "project_scope" } else { "user_input" },
        "confirmed": true,
        "confirmed_at": confirmed_at,
        "confirmed_by": user.user_id
    });
    let mut constraints = request.constraints.clone();
    if let Some(role_id) = request.role_id.as_deref().filter(|value| !value.trim().is_empty()) {
        if !crate::role_resolution::ensure_visible_role(&state.pool, tenant_id, role_id)
            .await
            .map_err(|error| AppError::internal(error.to_string()))?
        {
            return Err(AppError::bad_request("角色不存在或未启用"));
        }
        if let Some(object) = constraints.as_object_mut() {
            object.insert("role_id".into(), json!(role_id));
        } else {
            constraints = json!({"role_id": role_id});
        }
    }
    let message_id = Uuid::new_v4().to_string();
    let assistant_message_id = Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await.map_err(internal)?;

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
    .bind(&constraints)
    .bind(&request.budget)
    .bind(&proposal)
    .bind(&user.user_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

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

    // 创建 turn（用户消息由 begin_turn 一并写入），后台执行 Agent 工具循环，
    // 前端通过 /pent/turns/{turn_id}/stream 接收流式回复。
    let turn_request = kairos_platform_pent_api::AgentTurnRequest {
        session_id: id.clone(),
        content: content.to_string(),
        message_type: kairos_platform_pent_api::ConversationMessageType::Chat,
        client_request_id: format!("msg-{}", Uuid::new_v4()),
        attachments: vec![],
    };
    let runtime = crate::conversation::build_runtime(&state.pool, None);
    let (turn_id, _run_id) = runtime
        .begin_turn_only(&turn_request)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let pool = state.pool.clone();
    let turn_id_clone = turn_id.clone();
    let tasks = state.conversation_tasks.clone();
    let (abort_handle, abort_registration) = futures_util::future::AbortHandle::new_pair();
    tasks.register(turn_id.clone(), abort_handle);
    tokio::spawn(async move {
        let execution = async {
            let runtime = crate::conversation::build_runtime(&pool, None);
            if let Err(error) = runtime.execute_turn(turn_id_clone.clone(), &turn_request).await {
                tracing::warn!(error = %error, "conversation turn execution failed");
            }
        };
        let aborted = futures_util::future::Abortable::new(execution, abort_registration)
            .await
            .is_err();
        tasks.remove(&turn_id_clone);
        if aborted {
            tracing::info!(turn_id = %turn_id_clone, "conversation turn execution aborted");
        }
    });

    Ok(Json(ApiResponse::new(json!({
        "id": turn_id,
        "turn_id": turn_id,
        "session_id": id,
        "status": "thinking"
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

    // 若会话绑定了项目且项目尚未配置目标范围，则回写对话确认的目标。
    if let Some(project_id) = sqlx::query_scalar::<_, Option<String>>(
        "SELECT project_id FROM pent_agent_sessions WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?
    {
        let scope = sqlx::query_scalar::<_, Value>(
            "SELECT asset_scope FROM pent_projects WHERE id=$1 AND tenant_id=$2 AND deleted=0",
        )
        .bind(&project_id)
        .bind(tenant_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?;
        if extract_scope_targets(scope.as_ref()).is_empty() {
            sqlx::query(
                "UPDATE pent_projects SET asset_scope=$2, updated_at=now()
                 WHERE id=$1 AND tenant_id=$3 AND deleted=0",
            )
            .bind(&project_id)
            .bind(serde_json::Value::Array(
                request.targets.iter().cloned().map(Value::String).collect(),
            ))
            .bind(tenant_id)
            .execute(&state.pool)
            .await
            .map_err(internal)?;
        }
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
    let requested_role_id = constraints.get("role_id").and_then(Value::as_str);
    let resolved_role = crate::role_resolution::resolve_role(
        &state.pool,
        tenant_id,
        requested_role_id,
        project_id.as_deref(),
    )
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;

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
    .bind(json!({
        "constraints": constraints,
        "role": resolved_role.audit_snapshot(requested_role_id),
    }))
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

    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 734291))")
        .bind(&run_id)
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
