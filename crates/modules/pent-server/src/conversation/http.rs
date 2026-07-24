//! LLM-06: Conversation HTTP endpoints.
use crate::PentState;
use crate::conversation::stream::turn_sse;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use futures_util::future::{AbortHandle, Abortable};
use kairos_platform_framework_web::AppError;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationMessageType};
use serde_json::{Value, json};

pub fn routes() -> Router<PentState> {
    Router::new()
        .route(
            "/pent/sessions/{id}/turns",
            get(list_turns).post(create_turn),
        )
        .route("/pent/sessions/{id}/events", get(session_events))
        .route("/pent/turns/{id}/cancel", post(cancel_turn))
        .route("/pent/turns/{id}/retry", post(retry_turn))
        .route("/pent/turns/{id}/stream", get(stream_turn))
}

async fn list_turns(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('id',t.id,'status',t.status,'intent',t.intent,'error_code',t.error_code,'error_message',t.error_message,'retryable',t.retryable,'created_at',t.created_at,'messages',COALESCE((SELECT jsonb_agg(jsonb_build_object('role',m.role,'content',m.content,'status',m.status,'format',m.format,'sequence',m.sequence) ORDER BY m.sequence) FROM pent_conversation_messages m WHERE m.turn_id=t.id),'[]'::jsonb)) FROM pent_conversation_turns t WHERE t.session_id=$1 ORDER BY t.created_at")
        .bind(&session_id).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn create_turn(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
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

async fn session_events(
    State(s): State<PentState>,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('id',m.id,'role',m.role,'content',m.content,'turn_id',m.turn_id,'created_at',m.created_at) FROM pent_conversation_messages m WHERE m.session_id=$1 ORDER BY m.created_at LIMIT 200")
        .bind(&session_id).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn cancel_turn(
    State(s): State<PentState>,
    Path(turn_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
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
) -> Result<Json<ApiResponse<Value>>, AppError> {
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
) -> axum::response::sse::Sse<
    impl futures_util::stream::Stream<
        Item = Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    turn_sse(s.pool.clone(), turn_id)
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
