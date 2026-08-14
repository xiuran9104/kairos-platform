//! Non-blocking SSE stream for real-time turn progress.
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{Stream, stream};
use kairos_platform_framework_database::PgPool;
use std::convert::Infallible;
use std::time::Duration;

struct StreamState {
    pool: PgPool,
    turn_id: String,
    last_sequence: i32,
    finished: bool,
}

pub fn turn_sse(
    pool: PgPool,
    turn_id: String,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let state = StreamState {
        pool,
        turn_id,
        last_sequence: -1,
        finished: false,
    };
    let events = stream::unfold(state, |mut state| async move {
        if state.finished {
            return None;
        }

        // Check turn status
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT status, error_message FROM pent_conversation_turns WHERE id=$1")
                .bind(&state.turn_id)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();

        match &row {
            None => {
                state.finished = true;
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(serde_json::json!({"error": "turn not found"}).to_string())),
                    state,
                ));
            }
            Some((status, _error_msg)) if status == "failed" => {
                state.finished = true;
                let error = _error_msg.clone().unwrap_or_else(|| "turn failed".into());
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(serde_json::json!({"error": error}).to_string())),
                    state,
                ));
            }
            Some((status, _)) if status == "cancelled" => {
                state.finished = true;
                return Some((
                    Ok(Event::default().event("cancelled").data("[CANCELLED]")),
                    state,
                ));
            }
            _ => {}
        }

        // Read one message at a time. Advancing the cursor past an entire batch
        // while emitting only its last row would silently drop intermediate
        // assistant/tool events.
        let message: Option<(String, String, String, String, i32)> = sqlx::query_as(
            "SELECT id, role, content, format, sequence
             FROM pent_conversation_messages
             WHERE turn_id=$1 AND sequence > $2
             ORDER BY sequence ASC
             LIMIT 1",
        )
        .bind(&state.turn_id)
        .bind(state.last_sequence)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();

        if let Some((_id, role, content, format, seq)) = message {
            state.last_sequence = seq;

            // Determine event type from role and format
            let event_type = match (role.as_str(), format.as_str()) {
                ("assistant", "tool_calls") => "conversation.tool_calls",
                ("tool", _) => "conversation.tool_result",
                ("assistant", _) => "conversation.assistant_message",
                _ => "conversation.message",
            };

            let data = serde_json::json!({
                "role": role,
                "content": content,
                "format": format,
                "sequence": seq
            });

            return Some((
                Ok(Event::default()
                    .event(event_type)
                    .id(seq.to_string())
                    .data(data.to_string())),
                state,
            ));
        }

        // No new messages. Check if turn is completed.
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM pent_conversation_turns WHERE id=$1 AND status IN ('completed','failed','cancelled')",
        )
        .bind(&state.turn_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();

        if status.is_some() {
            state.finished = true;
            return Some((Ok(Event::default().event("done").data("[DONE]")), state));
        }

        // Still thinking — heartbeat and wait
        tokio::time::sleep(Duration::from_millis(500)).await;
        Some((Ok(Event::default().comment("heartbeat")), state))
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}
