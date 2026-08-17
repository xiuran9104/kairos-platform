use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/settings/terminal/sessions", post(create_session))
        .route(
            "/pent/settings/terminal/sessions/{id}",
            delete(close_session),
        )
        .route(
            "/pent/settings/terminal/sessions/{id}/input",
            post(write_input),
        )
        .route(
            "/pent/settings/terminal/sessions/{id}/output",
            get(read_output),
        )
}

fn bus() -> Result<crate::transport::mq::NatsHandle, AppError> {
    crate::transport::mq::conversation_tool_bus()
        .ok_or_else(|| AppError::internal("pent-agent 终端通道不可用"))
}

async fn require_owner(
    pool: &kairos_platform_framework_database::PgPool,
    id: &str,
    user: &CurrentUser,
) -> Result<(), AppError> {
    let tenant_id = security::tenant_id(user)?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT COALESCE((
             SELECT creator=$3 AND sub_type NOT IN ('closed','expired','worker_lost')
             FROM system_operate_log
             WHERE type='pent_terminal' AND trace_id=$1 AND tenant_id=$2 AND deleted=0
             ORDER BY id DESC LIMIT 1
         ),false)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(&user.user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if allowed {
        Ok(())
    } else {
        Err(AppError::not_found("终端会话不存在或已关闭"))
    }
}

async fn request(
    subject: &str,
    request: crate::transport::mq::TerminalRequest,
) -> Result<crate::transport::mq::TerminalResponse, AppError> {
    let response = bus()?
        .terminal_request(subject, &request)
        .await
        .map_err(AppError::internal)?;
    if let Some(error) = response.error.as_deref() {
        return Err(AppError::internal(error));
    }
    Ok(response)
}

async fn session_request(
    state: &PentState,
    id: &str,
    user: &CurrentUser,
    request_payload: crate::transport::mq::TerminalRequest,
) -> Result<crate::transport::mq::TerminalResponse, AppError> {
    let result = bus()?
        .terminal_request(
            &crate::transport::mq::subjects::terminal_session(id),
            &request_payload,
        )
        .await;
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            let tenant_id = security::tenant_id(user)?;
            if let Err(audit_error) = sqlx::query(
                "INSERT INTO system_operate_log(
                     id,trace_id,user_id,user_type,type,sub_type,biz_id,action,success,
                     extra,request_method,request_url,creator,updater,tenant_id
                 ) VALUES(nextval('system_operate_log_seq'),$1,0,0,'pent_terminal',
                          'worker_lost',0,'terminal.worker_lost',false,$2,'NATS',
                          'penta.terminal',$3,$3,$4)",
            )
            .bind(id)
            .bind(json!({"session_id":id,"error":error}).to_string())
            .bind(&user.user_id)
            .bind(tenant_id)
            .execute(&state.pool)
            .await
            {
                tracing::warn!(%audit_error, session_id = id, "终端 Worker 丢失审计写入失败");
            }
            return Err(AppError::internal(error));
        }
    };
    if let Some(error) = response.error.as_deref() {
        return Err(AppError::internal(error));
    }
    Ok(response)
}

async fn create_session(
    State(state): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::require(&user, "pent:tool:manage")?;
    let tenant_id = security::tenant_id(&user)?;
    let active_sessions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT DISTINCT ON (trace_id) trace_id,creator,sub_type
             FROM system_operate_log
             WHERE type='pent_terminal' AND tenant_id=$1 AND deleted=0
             ORDER BY trace_id,id DESC
         ) latest
         WHERE creator=$2 AND sub_type NOT IN ('closed','expired','worker_lost')",
    )
    .bind(tenant_id)
    .bind(&user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if active_sessions >= 5 {
        return Err(AppError::bad_request("每个用户最多同时创建 5 个终端会话"));
    }
    let response = request(
        crate::transport::mq::subjects::TERMINAL_CREATE,
        crate::transport::mq::TerminalRequest {
            action: "create".into(),
            session_id: None,
            data: None,
            after: None,
            actor: Some(user.user_id.clone()),
            tenant_id: Some(tenant_id),
        },
    )
    .await?;
    let id = response
        .session_id
        .ok_or_else(|| AppError::internal("pent-agent 未返回终端 session_id"))?;
    Ok(Json(ApiResponse::new(json!({"id":id}))))
}

async fn write_input(
    Path(id): Path<String>,
    State(state): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::require(&user, "pent:tool:manage")?;
    require_owner(&state.pool, &id, &user).await?;
    let data = body["data"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("data 不能为空"))?;
    session_request(
        &state,
        &id,
        &user,
        crate::transport::mq::TerminalRequest {
            action: "input".into(),
            session_id: Some(id.clone()),
            data: Some(data.into()),
            after: None,
            actor: None,
            tenant_id: None,
        },
    )
    .await?;
    Ok(Json(ApiResponse::new(json!({"ok":true}))))
}

#[derive(Deserialize)]
struct OutputQuery {
    after: Option<u64>,
}

async fn read_output(
    Path(id): Path<String>,
    Query(query): Query<OutputQuery>,
    State(state): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::require(&user, "pent:tool:manage")?;
    require_owner(&state.pool, &id, &user).await?;
    let response = session_request(
        &state,
        &id,
        &user,
        crate::transport::mq::TerminalRequest {
            action: "output".into(),
            session_id: Some(id.clone()),
            data: None,
            after: query.after,
            actor: None,
            tenant_id: None,
        },
    )
    .await?;
    Ok(Json(ApiResponse::new(json!({
        "chunks": response.chunks,
        "sequence": response.sequence,
        "closed": response.closed
    }))))
}

async fn close_session(
    Path(id): Path<String>,
    State(state): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::require(&user, "pent:tool:manage")?;
    require_owner(&state.pool, &id, &user).await?;
    let result = session_request(
        &state,
        &id,
        &user,
        crate::transport::mq::TerminalRequest {
            action: "close".into(),
            session_id: Some(id.clone()),
            data: None,
            after: None,
            actor: None,
            tenant_id: None,
        },
    )
    .await;
    result?;
    Ok(Json(ApiResponse::new(json!({"closed":true}))))
}
