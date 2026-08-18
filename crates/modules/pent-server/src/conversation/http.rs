//! Streaming and report endpoints for the canonical `/pent/*` API.
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
        .route("/pent/tasks/{id}/stream", get(stream_pent_task))
        .route("/pent/reports/{id}", get(get_report))
        // Keep turn SSE for live monitoring; cancel turn for abort
        .route("/pent/turns/{id}/stream", get(stream_turn))
        .route("/pent/turns/{id}/cancel", post(cancel_turn))
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
