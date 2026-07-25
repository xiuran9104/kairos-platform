//! PostgreSQL conversation persistence with idempotent turn creation.
use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{
    AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
};
use sha2::{Digest, Sha256};

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

    async fn ensure_execution_run(
        &self,
        turn_id: &str,
        session_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let mut tx = self.pool.begin().await?;
        let session: (
            i64,
            Option<String>,
            String,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            String,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT tenant_id,project_id,objective,
                    COALESCE(confirmed_scope_snapshot,'{}'::jsonb),
                    constraints,budget,created_by,active_run_id
             FROM pent_agent_sessions WHERE id=$1 FOR UPDATE",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Pent 会话不存在"))?;
        if let Some(run_id) = session.7 {
            sqlx::query("UPDATE pent_conversation_turns SET run_id=$2 WHERE id=$1")
                .bind(turn_id)
                .bind(&run_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(Some(run_id));
        }
        let authorized = session
            .3
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|targets| !targets.is_empty());
        if !authorized {
            tx.commit().await?;
            return Ok(None);
        }

        let task_id = uuid::Uuid::new_v4().to_string();
        let run_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO pent_tasks(
                 id,tenant_id,project_id,name,description,target_scope,status,created_by
             ) VALUES($1,$2,$3,$4,$4,$5,'running',$6)",
        )
        .bind(&task_id)
        .bind(session.0)
        .bind(&session.1)
        .bind(&session.2)
        .bind(&session.3)
        .bind(&session.6)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO pent_runs(
                 id,task_id,tenant_id,user_id,project_id,status,started_at,session_id,
                 objective_snapshot,scope_snapshot,policy_snapshot,budget_snapshot
             ) VALUES($1,$2,$3,$4,$5,'running',now(),$6,$7,$8,$9,$10)",
        )
        .bind(&run_id)
        .bind(&task_id)
        .bind(session.0)
        .bind(&session.6)
        .bind(&session.1)
        .bind(session_id)
        .bind(&session.2)
        .bind(&session.3)
        .bind(&session.4)
        .bind(&session.5)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE pent_agent_sessions
             SET active_run_id=$2,status='running',updated_at=now() WHERE id=$1",
        )
        .bind(session_id)
        .bind(&run_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE pent_conversation_turns SET run_id=$2 WHERE id=$1")
            .bind(turn_id)
            .bind(&run_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(run_id))
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

    async fn record_tool_result(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
        success: bool,
        content: &str,
        duration_ms: u64,
    ) -> anyhow::Result<String> {
        let evidence_id = uuid::Uuid::new_v4().to_string();
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .unwrap_or_else(|_| serde_json::json!({"invalid_arguments": true}));
        let content_value = serde_json::from_str::<serde_json::Value>(content).ok();
        let executor_backend = if content_value
            .as_ref()
            .and_then(|value| value.get("blocked"))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            "policy"
        } else if matches!(tool_name, "js_analyze" | "knowledge_search") {
            "builtin"
        } else {
            "docker"
        };
        let hash = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pent_tool_calls(
                 id,run_id,tool_name,params_summary,permission_decision,
                 started_at,completed_at,duration_ms,exit_code,executor_backend,execution_audit
             ) VALUES($1,$2,$3,$4,'allowed',now(),now(),$5,$6,$7,$8)
             ON CONFLICT(id) DO UPDATE SET
                 completed_at=now(),duration_ms=excluded.duration_ms,
                 exit_code=excluded.exit_code,execution_audit=excluded.execution_audit",
        )
        .bind(tool_call_id)
        .bind(run_id)
        .bind(tool_name)
        .bind(redact_arguments(arguments.clone()))
        .bind(i64::try_from(duration_ms).unwrap_or(i64::MAX))
        .bind(if success { 0 } else { 1 })
        .bind(executor_backend)
        .bind(serde_json::json!({
            "backend": executor_backend,
            "successful_observation": success
        }))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO pent_evidence(
                 id,run_id,tool_call_id,source,content_hash,content_type
             ) VALUES($1,$2,$3,$4,$5,'application/json')",
        )
        .bind(&evidence_id)
        .bind(run_id)
        .bind(tool_call_id)
        .bind(content)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
        if success {
            let memory_content = serde_json::json!({
                "tool": tool_name,
                "outcome": "success",
                "evidence_id": evidence_id,
                "arguments": redact_sensitive_value(arguments.clone()),
                "observation": compact_memory_observation(content_value.as_ref()),
            })
            .to_string();
            sqlx::query(
                "INSERT INTO pent_knowledge_documents(
                     doc_type,title,content,source,tags,answer_type,manual
                 ) VALUES('answer',$1,$2,'conversation-memory',$3,'tool_result',false)",
            )
            .bind(format!("{tool_name} tool observation"))
            .bind(memory_content)
            .bind(serde_json::json!([
                "conversation-memory",
                run_id,
                tool_name
            ]))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(evidence_id)
    }

    async fn checkpoint_context(
        &self,
        turn_id: &str,
        context: &ConversationContext,
        run_summary: &str,
        evidence_ids: &[String],
    ) -> anyhow::Result<()> {
        let message_ids: Vec<&str> = context
            .recent_messages
            .iter()
            .map(|message| message.message_id.as_str())
            .collect();
        let hash_input = serde_json::json!({
            "turn_id": turn_id,
            "run_summary": run_summary,
            "message_ids": message_ids,
            "evidence_ids": evidence_ids,
            "memory_ids": context.memory_refs,
        });
        let content_hash = format!("{:x}", Sha256::digest(hash_input.to_string().as_bytes()));
        let next_version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(context_version),0)+1
             FROM pent_context_snapshots WHERE turn_id=$1",
        )
        .bind(turn_id)
        .fetch_one(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO pent_context_snapshots(
                 id,turn_id,session_id,run_id,context_version,objective_snapshot,
                 scope_snapshot,policy_snapshot,budget_snapshot,run_summary,
                 message_ids,evidence_ids,memory_ids,tool_ids,estimated_tokens,content_hash
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(turn_id)
        .bind(&context.session_id)
        .bind(&context.run_id)
        .bind(next_version)
        .bind(&context.objective_snapshot)
        .bind(&context.scope_snapshot)
        .bind(&context.policy_snapshot)
        .bind(&context.budget_snapshot)
        .bind(run_summary)
        .bind(serde_json::to_value(message_ids)?)
        .bind(serde_json::to_value(evidence_ids)?)
        .bind(serde_json::to_value(&context.memory_refs)?)
        .bind(serde_json::to_value(&context.tool_ids)?)
        .bind(i64::try_from(context.estimated_tokens).unwrap_or(i64::MAX))
        .bind(content_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn redact_arguments(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(object) = value.as_object_mut() {
        for (key, item) in object {
            if matches!(
                key.to_ascii_lowercase().as_str(),
                "authorization" | "cookie" | "password" | "token" | "csrf_token"
            ) {
                *item = serde_json::Value::String("[REDACTED]".into());
            }
        }
    }
    value
}

fn redact_sensitive_value(mut value: serde_json::Value) -> serde_json::Value {
    match &mut value {
        serde_json::Value::Object(object) => {
            for (key, item) in object {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "authorization"
                        | "cookie"
                        | "set-cookie"
                        | "password"
                        | "token"
                        | "csrf_token"
                        | "api_key"
                ) {
                    *item = serde_json::Value::String("[REDACTED]".into());
                } else {
                    *item = redact_sensitive_value(item.take());
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                *item = redact_sensitive_value(item.take());
            }
        }
        _ => {}
    }
    value
}

fn compact_memory_observation(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(value) = value else {
        return serde_json::json!({"available_in_evidence": true});
    };
    let redacted = redact_sensitive_value(value.clone());
    let serialized = redacted.to_string();
    if serialized.chars().count() <= 2_000 {
        return redacted;
    }
    serde_json::json!({
        "preview": serialized.chars().take(2_000).collect::<String>(),
        "truncated": true,
        "available_in_evidence": true
    })
}
