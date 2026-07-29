//! LLM-06: Conversation HTTP endpoints.
use crate::PentState;
use crate::conversation::ports::ConversationAuthorizationService;
use crate::conversation::stream::turn_sse;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Response, header},
    routing::{get, post},
};
use futures_util::{future::{AbortHandle, Abortable}, stream};
use kairos_platform_ai_api::ChatMessage;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationMessageType};
use serde_json::{Value, json};
use tokio::sync::mpsc;

pub fn routes() -> Router<PentState> {
    Router::new()
        .route(
            "/pent/sessions/{id}/turns",
            get(list_turns).post(create_turn),
        )
        .route(
            "/pent/sessions/{id}/turns/stream",
            post(create_turn_stream),
        )
        .route("/pent/sessions/{id}/events", get(session_events))
        .route("/pent/turns/{id}/cancel", post(cancel_turn))
        .route("/pent/turns/{id}/retry", post(retry_turn))
        .route("/pent/turns/{id}/stream", get(stream_turn))
}

async fn list_turns(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    crate::security::ensure_session_access(&s.pool, &session_id, &user).await?;
    let rows = sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('id',t.id,'status',t.status,'intent',t.intent,'error_code',t.error_code,'error_message',t.error_message,'retryable',t.retryable,'created_at',t.created_at,'messages',COALESCE((SELECT jsonb_agg(jsonb_build_object('role',m.role,'content',m.content,'status',m.status,'format',m.format,'sequence',m.sequence) ORDER BY m.sequence) FROM pent_conversation_messages m WHERE m.turn_id=t.id),'[]'::jsonb)) FROM pent_conversation_turns t WHERE t.session_id=$1 ORDER BY t.created_at")
        .bind(&session_id).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn create_turn(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::security::ensure_session_access(&s.pool, &session_id, &user).await?;
    let content = body["content"].as_str().unwrap_or("").trim().to_string();
    if content.is_empty() {
        return Err(AppError::bad_request("消息内容不能为空"));
    }
    let client_req_id = body
        .get("client_request_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Check idempotency
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM pent_conversation_turns WHERE session_id=$1 AND client_request_id=$2",
    )
    .bind(&session_id)
    .bind(&client_req_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    {
        return Ok(Json(ApiResponse::new(
            json!({"turn_id":existing,"status":"already_exists"}),
        )));
    }

    let message_type: ConversationMessageType = body
        .get("message_type")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| AppError::bad_request("不支持的消息类型"))?
        .unwrap_or(ConversationMessageType::Chat);
    let request = AgentTurnRequest {
        session_id,
        content,
        message_type,
        client_request_id: client_req_id,
        attachments: vec![],
    };

    // Create the turn and user message synchronously, then run the tool loop
    // in a background task so the HTTP response returns immediately.
    let pool = s.pool.clone();
    let rt = crate::conversation::build_runtime(&pool);
    let (turn_id, _run_id) = rt
        .begin_turn_only(&request)
        .await
        .map_err(|error| AppError::internal(format!(" Pent 对话处理失败: {error}")))?;

    // Register the abort handle before spawning so a cancellation request can
    // stop model calls and tools, instead of only changing the database status.
    let bg_turn_id = turn_id.clone();
    let cleanup_turn_id = turn_id.clone();
    let task_registry = s.conversation_tasks.clone();
    let cleanup_registry = task_registry.clone();
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    task_registry.register(turn_id.clone(), abort_handle);
    tokio::spawn(async move {
        match Abortable::new(rt.execute_turn(bg_turn_id, &request), abort_registration).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(error = %error, "conversation turn failed");
            }
            Err(_) => {
                tracing::info!(turn_id = %cleanup_turn_id, "conversation turn aborted");
            }
        }
        cleanup_registry.remove(&cleanup_turn_id);
    });

    Ok(Json(ApiResponse::new(json!({
        "turn_id": turn_id,
        "status": "thinking",
        "processing": true
    }))))
}

/// Stream a simple chat turn token-by-token over SSE.
///
/// Unlike `create_turn`, this endpoint does **not** run the autonomous Pent tool
/// loop. It is meant for plain conversational messages and mirrors the UX of
/// `/ai/chat/message/send-stream` while still persisting messages under the
/// agent session/turn model.
async fn create_turn_stream(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Response<Body>, AppError> {
    crate::security::ensure_session_access(&s.pool, &session_id, &user).await?;
    let content = body["content"].as_str().unwrap_or("").trim().to_string();
    if content.is_empty() {
        return Err(AppError::bad_request("消息内容不能为空"));
    }
    let client_req_id = body
        .get("client_request_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Idempotency: replay the existing turn id to the client and close.
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM pent_conversation_turns WHERE session_id=$1 AND client_request_id=$2",
    )
    .bind(&session_id)
    .bind(&client_req_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    {
        let body = Body::from(format!(
            "data: {}\n\nevent: done\ndata: [DONE]\n\n",
            json!({"type":"already_exists","turn_id":existing})
        ));
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body)
            .map_err(|_| AppError::internal("创建流失败"));
    }

    // Streaming chat is only for plain conversation without an active run.
    let active_run_id: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT active_run_id FROM pent_agent_sessions WHERE id=$1",
    )
    .bind(&session_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .flatten();
    if active_run_id.is_some() {
        return Err(AppError::bad_request(
            "当前会话有运行中的任务，请使用非流式端点",
        ));
    }

    // Try auto-authorization: detect explicit user declaration of authorised scope.
    let turn_id = uuid::Uuid::new_v4().to_string();
    let user_message_id = uuid::Uuid::new_v4().to_string();
    let assistant_message_id = uuid::Uuid::new_v4().to_string();

    // Apply authorization auto-detection before persisting messages so the session
    // transitions to "ready" when the user explicitly declares scope in chat.
    let auth_service =
        crate::conversation::authorization::PgConversationAuthorizationService::new(s.pool.clone());
    let _ = auth_service
        .reconcile_explicit_authorization(&session_id, &content)
        .await;

    // Persist turn, user message, and a placeholder streaming assistant message.
    sqlx::query(
        "INSERT INTO pent_conversation_turns(
             id, session_id, run_id, client_request_id, status, message_type,
             intent, intent_confidence, started_at
         ) VALUES($1,$2,NULL,$3,'streaming',$4,'chat',0.8,now())",
    )
    .bind(&turn_id)
    .bind(&session_id)
    .bind(&client_req_id)
    .bind("chat")
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    sqlx::query(
        "INSERT INTO pent_conversation_messages(
             id, session_id, turn_id, role, message_type, content, payload,
             created_by, sequence, status, format
         ) VALUES($1,$2,$3,'user',$4,$5,$6,'system',0,'completed','text')",
    )
    .bind(&user_message_id)
    .bind(&session_id)
    .bind(&turn_id)
    .bind("chat")
    .bind(&content)
    .bind(json!({"attachments": []}))
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    sqlx::query(
        "INSERT INTO pent_conversation_messages(
             id, session_id, turn_id, role, message_type, content, payload,
             created_by, sequence, status, format
         ) VALUES($1,$2,$3,'assistant','chat','',$4,'system',1,'streaming','text')",
    )
    .bind(&assistant_message_id)
    .bind(&session_id)
    .bind(&turn_id)
    .bind(json!({}))
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    let gateway = crate::conversation::model_gateway::AiModelGateway::new(s.pool.clone());
    let (model_id, temperature, max_tokens) = gateway
        .resolve_model(&session_id)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    // Build a context-aware system prompt from session state.
    let session_row: Option<(String, Option<Value>)> = sqlx::query_as(
        "SELECT status, confirmed_scope_snapshot FROM pent_agent_sessions WHERE id=$1",
    )
    .bind(&session_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let has_active_run = sqlx::query_scalar::<_, bool>(
        "SELECT active_run_id IS NOT NULL FROM pent_agent_sessions WHERE id=$1",
    )
    .bind(&session_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .unwrap_or(false);

    let system_prompt = {
        let mut base = String::from(
            "你是 Pent 平台的自动化渗透测试智能体。你有以下能力：\n\
             - 执行信息收集（存活探测、指纹识别、端口扫描、目录枚举）\n\
             - 漏洞检测与验证（SQL注入、XSS、文件上传、SSRF等）\n\
             - 自动化攻击链构建\n\n",
        );
        if let Some((ref status, ref maybe_scope)) = session_row {
            base.push_str(&format!("当前会话状态: {status}\n"));
            if let Some(scope) = maybe_scope {
                if let Some(targets) = scope.get("targets").and_then(|t| t.as_array()) {
                    let target_list: Vec<&str> = targets.iter().filter_map(|t| t.as_str()).collect();
                    if !target_list.is_empty() {
                        base.push_str(&format!("已授权目标: {}\n", target_list.join(", ")));
                    }
                }
            }
            base.push_str(&format!("活跃 Run: {}\n\n", if has_active_run { "是" } else { "否" }));
        }
        base.push_str(
            "核心原则：\n\
             1. 用户说\"开始\"/\"继续\"/\"执行\"/\"直接\"时，立即进入行动模式，给具体操作步骤和命令\n\
             2. 不要反复确认授权（授权已在会话中确认过了）\n\
             3. 有活跃 Run 时直接调用工具，没有 Run 时给出立即可执行的命令\n\
             4. 简洁直接，先做再说，不要冗长的免责声明\n\
             5. 发现漏洞后直接报告漏洞类型 + 证据，不要只给手工操作指南\n\n\
             请用中文回答。",
        );
        base
    };

    let history: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM pent_conversation_messages
         WHERE session_id=$1 AND role IN ('user','assistant') AND status='completed'
         ORDER BY created_at DESC, sequence DESC LIMIT 20",
    )
    .bind(&session_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    let mut messages = vec![ChatMessage {
        role: "system".into(),
        content: system_prompt,
    }];
    for (role, content) in history.into_iter().rev() {
        messages.push(ChatMessage { role, content });
    }
    messages.push(ChatMessage {
        role: "user".into(),
        content: content.clone(),
    });

    let request = kairos_platform_ai_api::ChatRequest {
        model: String::new(),
        messages,
        temperature,
        max_tokens: Some(max_tokens),
    };

    let (tx, rx) = mpsc::channel::<Result<String, std::convert::Infallible>>(32);
    let pool = s.pool.clone();
    let turn_id_for_task = turn_id.clone();
    let assistant_message_id_for_task = assistant_message_id.clone();

    tokio::spawn(async move {
        let factory = kairos_platform_ai_server::AiModelFactory::new(pool.clone());
        let tx_inner = tx.clone();
        let full_content = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        let full_content_for_callback = full_content.clone();
        let pool_for_callback = pool.clone();
        let turn_id_for_callback = turn_id_for_task.clone();
        let assistant_message_id_for_callback = assistant_message_id_for_task.clone();

        let result = factory
            .chat_stream(model_id, request, move |delta: String| {
                let tx = tx_inner.clone();
                let full_content = full_content_for_callback.clone();
                let pool = pool_for_callback.clone();
                let turn_id = turn_id_for_callback.clone();
                let assistant_message_id = assistant_message_id_for_callback.clone();
                async move {
                    let mut content = full_content.lock().await;
                    content.push_str(&delta);
                    let content_clone = content.clone();
                    drop(content);

                    let _ = sqlx::query(
                        "UPDATE pent_conversation_messages SET content=$2 WHERE id=$1",
                    )
                    .bind(&assistant_message_id)
                    .bind(&content_clone)
                    .execute(&pool)
                    .await;

                    let payload = json!({
                        "type": "delta",
                        "turn_id": turn_id,
                        "message_id": assistant_message_id,
                        "content": delta,
                    });
                    tx.send(Ok(format!("data: {}\n\n", payload)))
                        .await
                        .map_err(|_| "客户端已断开".to_string())
                }
            })
            .await;

        match result {
            Ok(response) => {
                let final_content = response.content;
                let reasoning = response.reasoning;
                let _ = sqlx::query(
                    "UPDATE pent_conversation_messages SET content=$2, reasoning_summary=$3, status='completed' WHERE id=$1",
                )
                .bind(&assistant_message_id_for_task)
                .bind(&final_content)
                .bind(&reasoning)
                .execute(&pool)
                .await;
                let _ = sqlx::query(
                    "UPDATE pent_conversation_turns SET status='completed', completed_at=now() WHERE id=$1",
                )
                .bind(&turn_id_for_task)
                .execute(&pool)
                .await;

                let payload = json!({
                    "type": "complete",
                    "turn_id": turn_id_for_task,
                    "message_id": assistant_message_id_for_task,
                    "content": final_content,
                    "reasoning_content": reasoning,
                });
                let _ = tx.send(Ok(format!("data: {}\n\n", payload))).await;
                let _ = tx.send(Ok("event: done\ndata: [DONE]\n\n".to_string())).await;
            }
            Err(error) => {
                let error_msg = format!("{error:?}");
                let _ = sqlx::query(
                    "UPDATE pent_conversation_messages SET content=$2, status='failed' WHERE id=$1",
                )
                .bind(&assistant_message_id_for_task)
                .bind(&format!("生成失败: {error_msg}"))
                .execute(&pool)
                .await;
                let _ = sqlx::query(
                    "UPDATE pent_conversation_turns SET status='failed', error_code='STREAM_ERROR', error_message=$2, completed_at=now() WHERE id=$1",
                )
                .bind(&turn_id_for_task)
                .bind(&error_msg)
                .execute(&pool)
                .await;

                let payload = json!({"code":500,"msg":error_msg});
                let _ = tx.send(Ok(format!("event: error\ndata: {}\n\n", payload))).await;
            }
        }
    });

    let body = Body::from_stream(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }));
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .map_err(|_| AppError::internal("创建流失败"))
}

async fn session_events(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    crate::security::ensure_session_access(&s.pool, &session_id, &user).await?;
    let rows = sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('id',m.id,'role',m.role,'content',m.content,'turn_id',m.turn_id,'created_at',m.created_at) FROM pent_conversation_messages m WHERE m.session_id=$1 ORDER BY m.created_at LIMIT 200")
        .bind(&session_id).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn cancel_turn(
    State(s): State<PentState>,
    Path(turn_id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::security::ensure_turn_access(&s.pool, &turn_id, &user).await?;
    let affected = sqlx::query(
        "UPDATE pent_conversation_turns
         SET status='cancelled', completed_at=now()
         WHERE id=$1 AND status IN ('queued','thinking','streaming')",
    )
    .bind(&turn_id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .rows_affected();

    if affected == 0 {
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM pent_conversation_turns WHERE id=$1",
        )
        .bind(&turn_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("对话轮次不存在"))?;

        if status != "cancelled" {
            return Err(AppError::bad_request(format!(
                "状态为 {status} 的对话无法中止"
            )));
        }
    }

    let execution_aborted = s.conversation_tasks.abort(&turn_id);
    Ok(Json(ApiResponse::new(json!({
        "turn_id": turn_id,
        "status": "cancelled",
        "execution_aborted": execution_aborted
    }))))
}

async fn retry_turn(
    State(s): State<PentState>,
    Path(turn_id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::security::ensure_turn_access(&s.pool, &turn_id, &user).await?;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT t.session_id, t.message_type, m.content
         FROM pent_conversation_turns t
         JOIN pent_conversation_messages m
           ON m.turn_id=t.id AND m.role='user'
         WHERE t.id=$1 AND t.status='failed' AND t.retryable=true
         ORDER BY m.sequence LIMIT 1",
    )
    .bind(&turn_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let (session_id, message_type, content) =
        row.ok_or_else(|| AppError::bad_request("该对话不可重试"))?;
    let message_type = serde_json::from_value(Value::String(message_type))
        .map_err(|_| AppError::internal("原始消息类型无效"))?;
    let request = AgentTurnRequest {
        session_id,
        content,
        message_type,
        client_request_id: format!("retry-{}", uuid::Uuid::new_v4()),
        attachments: vec![],
    };
    let result = crate::conversation::build_runtime(&s.pool)
        .handle(&request)
        .await
        .map_err(|error| AppError::internal(format!(" Pent 对话重试失败: {error}")))?;
    Ok(Json(ApiResponse::new(json!({
        "original_turn_id": turn_id,
        "turn_id": result.turn_id,
        "status": "completed"
    }))))
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
