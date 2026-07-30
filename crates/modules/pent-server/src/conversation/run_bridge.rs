use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationMessageType};
use serde_json::json;

/// Execute a session-backed Run through the same autonomous conversation
/// runtime used by the UI. Returns `false` for legacy task-only runs.
pub(crate) async fn execute_session_run(
    pool: &PgPool,
    run_id: &str,
    worker_id: &str,
) -> Result<bool, String> {
    let session: Option<(String, String)> = sqlx::query_as(
        "SELECT session_id, objective_snapshot
         FROM pent_runs
         WHERE id = $1 AND session_id IS NOT NULL",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some((session_id, objective)) = session else {
        return Ok(false);
    };

    append_event(
        pool,
        run_id,
        "run.autonomous.started",
        json!({
            "worker_id": worker_id,
            "session_id": session_id,
            "runtime": "conversation-tool-loop"
        }),
    )
    .await?;

    let request = AgentTurnRequest {
        session_id: session_id.clone(),
        content: objective,
        message_type: ConversationMessageType::Chat,
        client_request_id: format!("run-{run_id}"),
        attachments: vec![],
    };
    let runtime = super::build_runtime(pool, None);
    match runtime.handle(&request).await {
        Ok(result) => {
            let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE pent_runs
                 SET status='completed', completed_at=now(), lease_expires_at=NULL,
                     updated_at=now()
                 WHERE id=$1 AND status='running'",
            )
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE pent_agent_sessions
                 SET status='completed', active_run_id=NULL, updated_at=now()
                 WHERE id=$1 AND active_run_id=$2",
            )
            .bind(&session_id)
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
            tx.commit().await.map_err(|error| error.to_string())?;
            match crate::report::generate_run_report(
                pool,
                run_id,
                Some(&result.message.content),
            )
            .await
            {
                Ok(artifact) => {
                    append_event(
                        pool,
                        run_id,
                        "run.report.completed",
                        json!({
                            "turn_id": result.turn_id,
                            "message_id": result.message.message_id,
                            "artifact_id": artifact.artifact_id,
                            "object_key": artifact.object_key,
                            "size_bytes": artifact.size_bytes,
                            "sha256": artifact.sha256,
                            "report": result.message.content
                        }),
                    )
                    .await?;
                }
                Err(error) => {
                    tracing::error!(run_id = %run_id, error = %error, "run report generation failed");
                    append_event(
                        pool,
                        run_id,
                        "run.report.failed",
                        json!({
                            "turn_id": result.turn_id,
                            "message_id": result.message.message_id,
                            "error": error
                        }),
                    )
                    .await?;
                }
            }
            Ok(true)
        }
        Err(error) => {
            let message = error.to_string();
            let blocked = message.starts_with("RUN_BLOCKED:")
                || message.starts_with("RUN_INCOMPLETE:")
                || message.contains("budget exhausted");
            let terminal_status = if blocked { "blocked" } else { "failed" };
            let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE pent_runs
                 SET status=$2, failure_code=$3, failure_message=$4,
                     completed_at=now(), lease_expires_at=NULL,
                     updated_at=now()
                 WHERE id=$1 AND status='running'",
            )
            .bind(run_id)
            .bind(terminal_status)
            .bind(if blocked {
                "incomplete"
            } else {
                "conversation_failed"
            })
            .bind(&message)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE pent_agent_sessions
                 SET status=$3, active_run_id=NULL, updated_at=now()
                 WHERE id=$1 AND active_run_id=$2",
            )
            .bind(&session_id)
            .bind(run_id)
            .bind(terminal_status)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
            tx.commit().await.map_err(|error| error.to_string())?;
            append_event(pool, run_id, "run.failed", json!({"error": message})).await?;
            Err(message)
        }
    }
}

async fn append_event(
    pool: &PgPool,
    run_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO pent_run_events(
             event_id, run_id, sequence, event_type, occurred_at, payload
         )
         SELECT $1, $2, COALESCE(MAX(sequence), -1) + 1, $3, now(), $4
         FROM pent_run_events WHERE run_id = $2",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}
