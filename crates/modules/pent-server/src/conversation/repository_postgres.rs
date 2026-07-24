//! PostgreSQL conversation persistence with idempotent turn creation.
use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{AgentTurnRequest, AgentTurnResult, ConversationIntent};

use crate::conversation::ports::ConversationRepository;

pub struct PgConversationRepository {
    pool: PgPool,
}

impl PgConversationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConversationRepository for PgConversationRepository {
    async fn begin_turn(
        &self,
        request: &AgentTurnRequest,
        intent: &ConversationIntent,
    ) -> anyhow::Result<(String, Option<String>)> {
        let mut tx = self.pool.begin().await?;
        let run_id: Option<String> = sqlx::query_scalar(
            "SELECT active_run_id FROM pent_agent_sessions WHERE id = $1 FOR UPDATE",
        )
        .bind(&request.session_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow::anyhow!(" Pent 会话不存在"))?;
        if let Some(existing) = sqlx::query_scalar::<_, String>(
            "SELECT id FROM pent_conversation_turns
             WHERE session_id = $1 AND client_request_id = $2",
        )
        .bind(&request.session_id)
        .bind(&request.client_request_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok((existing, run_id));
        }
        let turn_id = uuid::Uuid::new_v4().to_string();
        let intent_kind = serde_json::to_value(&intent.kind)?
            .as_str()
            .unwrap_or("chat")
            .to_owned();
        let message_type = serde_json::to_value(&request.message_type)?
            .as_str()
            .unwrap_or("chat")
            .to_owned();
        sqlx::query(
            "INSERT INTO pent_conversation_turns(
                 id, session_id, run_id, client_request_id, status, message_type,
                 intent, intent_confidence, started_at
             ) VALUES($1,$2,$3,$4,'thinking',$5,$6,$7,now())",
        )
        .bind(&turn_id)
        .bind(&request.session_id)
        .bind(&run_id)
        .bind(&request.client_request_id)
        .bind(message_type)
        .bind(intent_kind)
        .bind(intent.confidence)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO pent_conversation_messages(
                 id, session_id, turn_id, role, message_type, content, payload,
                 created_by, sequence, status, format
             ) VALUES($1,$2,$3,'user',$4,$5,$6,'admin',0,'completed','text')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&request.session_id)
        .bind(&turn_id)
        .bind(
            serde_json::to_value(&request.message_type)?
                .as_str()
                .unwrap_or("chat"),
        )
        .bind(&request.content)
        .bind(serde_json::json!({"attachments": request.attachments}))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((turn_id, run_id))
    }

    async fn complete_turn(&self, result: &AgentTurnResult) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE pent_conversation_turns
             SET status='completed', model_role=$2, model_id=$3, input_tokens=$4,
                 output_tokens=$5, latency_ms=$6, completed_at=now()
             WHERE id=$1 AND status IN ('thinking','streaming')",
        )
        .bind(&result.turn_id)
        .bind(&result.usage.model_role)
        .bind(&result.usage.model_id)
        .bind(result.usage.input_tokens as i64)
        .bind(result.usage.output_tokens as i64)
        .bind(result.usage.latency_ms as i64)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 0 {
            anyhow::bail!("对话已结束或已取消");
        }
        sqlx::query(
            "INSERT INTO pent_conversation_messages(
                 id, session_id, turn_id, role, message_type, content, payload,
                 created_by, sequence, status, format, model_id, citations, usage
             ) VALUES($1,$2,$3,'assistant','chat',$4,$5,'system',1,'completed',
                      $6,$7,$8,$9)",
        )
        .bind(&result.message.message_id)
        .bind(&result.session_id)
        .bind(&result.turn_id)
        .bind(&result.message.content)
        .bind(serde_json::json!({"plan_update": result.plan_update}))
        .bind(&result.message.format)
        .bind(&result.usage.model_id)
        .bind(serde_json::to_value(&result.citations)?)
        .bind(serde_json::to_value(&result.usage)?)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO pent_model_calls(
                 id, turn_id, session_id, run_id, model_role, model_config_id,
                 model, status, input_tokens, output_tokens, latency_ms, completed_at
             ) VALUES($1,$2,$3,$4,$5,$6,$7,'completed',$8,$9,$10,now())",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&result.turn_id)
        .bind(&result.session_id)
        .bind(&result.run_id)
        .bind(&result.usage.model_role)
        .bind(
            result
                .usage
                .model_id
                .as_deref()
                .and_then(|id| id.parse::<i64>().ok()),
        )
        .bind(&result.usage.model_id)
        .bind(result.usage.input_tokens as i64)
        .bind(result.usage.output_tokens as i64)
        .bind(result.usage.latency_ms as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn fail_turn(
        &self,
        turn_id: &str,
        code: &str,
        message: &str,
        retryable: bool,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE pent_conversation_turns
             SET status='failed', error_code=$2, error_message=$3,
                 retryable=$4, completed_at=now()
             WHERE id=$1 AND status IN ('queued','thinking','streaming')",
        )
        .bind(turn_id)
        .bind(code)
        .bind(message)
        .bind(retryable)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn write_intermediate_message(
        &self,
        turn_id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        format: &str,
    ) -> anyhow::Result<()> {
        // Get next sequence number for this turn
        let seq: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1
             FROM pent_conversation_messages
             WHERE turn_id = $1",
        )
        .bind(turn_id)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            "INSERT INTO pent_conversation_messages(
                 id, session_id, turn_id, role, message_type, content, payload,
                 created_by, sequence, status, format
             ) VALUES($1, $2, $3, $4, 'chat', $5, $6, 'system', $7, 'completed', $8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(session_id)
        .bind(turn_id)
        .bind(role)
        .bind(content)
        .bind(serde_json::json!({}))
        .bind(seq as i32)
        .bind(format)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
