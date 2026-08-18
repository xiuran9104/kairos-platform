//! Reconcile conversation state after a Gateway restart.
//!
//! Run ownership belongs to the execution worker. Gateway recovery must not
//! fail a turn while that worker still owns a live lease.

use kairos_platform_framework_database::PgPool;

const RESTART_RECOVERY_MESSAGE: &str = "网关重启自动恢复";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConversationRecoveryStats {
    pub repaired_live_turns: u64,
    pub failed_orphaned_turns: u64,
    pub reconciled_sessions: u64,
}

/// Restore falsely failed worker-owned turns, fail only Gateway-owned orphan
/// turns, and make the session projection agree with its active Run.
pub async fn recover_conversation_state(
    pool: &PgPool,
) -> anyhow::Result<ConversationRecoveryStats> {
    let mut tx = pool.begin().await?;

    // Older Gateway versions marked every old turn as failed, even when a
    // worker was actively renewing the associated Run lease. Repair only this
    // identifiable false transition so unrelated failures remain terminal.
    let repaired_live_turns = sqlx::query(
        "UPDATE pent_conversation_turns turn_row
         SET status=CASE
                 WHEN EXISTS (
                     SELECT 1 FROM pent_conversation_messages message
                     WHERE message.turn_id=turn_row.id
                       AND message.role IN ('assistant','tool')
                 ) THEN 'streaming'
                 ELSE 'thinking'
             END,
             error_code=NULL,error_message=NULL,retryable=false,completed_at=NULL
         FROM pent_runs run
         WHERE run.id=turn_row.run_id
           AND turn_row.status='failed'
           AND turn_row.error_message=$1
           AND run.status='running'
           AND run.worker_id IS NOT NULL
           AND run.lease_expires_at>now()",
    )
    .bind(RESTART_RECOVERY_MESSAGE)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // A turn without a Run is owned by the Gateway process itself. After a
    // restart there is no executor capable of completing it, so it is safe to
    // fail. Run-backed turns are deliberately excluded: worker lease recovery
    // is the authority for those.
    let failed_orphaned_turns = sqlx::query(
        "UPDATE pent_conversation_turns
         SET status='failed',error_code='GATEWAY_RESTARTED',error_message=$1,
             retryable=true,completed_at=now()
         WHERE run_id IS NULL
           AND status IN ('queued','thinking','streaming')
           AND created_at<now()-interval '30 seconds'",
    )
    .bind(RESTART_RECOVERY_MESSAGE)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let active_sessions = sqlx::query(
        "UPDATE pent_agent_sessions session
         SET status='running',updated_at=now()
         FROM pent_runs run
         WHERE session.active_run_id=run.id
           AND run.status IN ('queued','running','cancelling','waiting_approval')
           AND session.status<>'running'",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let terminal_sessions = sqlx::query(
        "UPDATE pent_agent_sessions session
         SET status=CASE
                 WHEN run.status IN ('completed','failed','blocked','cancelled') THEN run.status
                 ELSE 'ready'
             END,
             active_run_id=NULL,updated_at=now()
         FROM pent_runs run
         WHERE session.active_run_id=run.id
           AND run.status NOT IN ('queued','running','cancelling','waiting_approval')",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let missing_run_sessions = sqlx::query(
        "UPDATE pent_agent_sessions session
         SET status='ready',active_run_id=NULL,updated_at=now()
         WHERE active_run_id IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM pent_runs run WHERE run.id=session.active_run_id)",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(ConversationRecoveryStats {
        repaired_live_turns,
        failed_orphaned_turns,
        reconciled_sessions: active_sessions + terminal_sessions + missing_run_sessions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_message_stays_specific_to_the_legacy_false_transition() {
        assert_eq!(RESTART_RECOVERY_MESSAGE, "网关重启自动恢复");
    }
}
