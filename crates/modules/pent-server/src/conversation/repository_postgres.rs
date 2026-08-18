//! PostgreSQL conversation persistence with idempotent turn creation.
use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{
    AgentTurnRequest, AgentTurnResult, ConversationContext, ConversationIntent,
};
use sha2::{Digest, Sha256};

use kairos_platform_pent_runtime::ports::{
    ConversationRepository, HistoricalToolCall, RunToolHistory,
};

const MAX_HISTORY_TOOL_CALLS: i64 = 500;
const MAX_EVIDENCE_PAGE_SIZE: i64 = 50;
const MAX_EVIDENCE_VIEW_CHARS: usize = 12_000;

fn classify_persisted_tool_result(
    success: bool,
    content: Option<&serde_json::Value>,
) -> (&'static str, &'static str, Option<String>, bool) {
    let blocked = content
        .and_then(|value| value.get("blocked"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let error_code = content
        .and_then(|value| value.get("error_code"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let retryable = content
        .and_then(|value| value.get("retryable"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let outcome = if blocked {
        "blocked"
    } else {
        match error_code.as_deref() {
            Some("TOOL_CANCELLED") => "cancelled",
            Some("TOOL_TIMEOUT") => "timed_out",
            Some("WORKER_UNAVAILABLE") => "worker_unavailable",
            _ if success => "succeeded",
            _ => "failed",
        }
    };
    let permission = if blocked { "denied" } else { "allowed" };
    (outcome, permission, error_code, retryable)
}

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
            let reusable: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pent_runs WHERE id=$1 AND status IN ('queued','running'))",
            )
            .bind(&run_id)
            .fetch_one(&mut *tx)
            .await?;
            if reusable {
                sqlx::query("UPDATE pent_conversation_turns SET run_id=$2 WHERE id=$1")
                    .bind(turn_id)
                    .bind(&run_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE pent_agent_sessions SET status='running',updated_at=now() WHERE id=$1",
                )
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return Ok(Some(run_id));
            }
            sqlx::query("UPDATE pent_agent_sessions SET active_run_id=NULL WHERE id=$1")
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
        }
        let task_id = uuid::Uuid::new_v4().to_string();
        let run_id = uuid::Uuid::new_v4().to_string();
        let task_name = task_display_name(&session.2, &session.3);
        let authorized_scope = crate::engagement::authorized_scope(
            &self.pool,
            session.0,
            session.1.as_deref(),
            session.3.clone(),
        )
        .await
        .map_err(anyhow::Error::msg)?;
        sqlx::query(
            "INSERT INTO pent_tasks(
                 id,tenant_id,project_id,name,description,target_scope,status,created_by
             ) VALUES($1,$2,$3,$4,$5,$6,'running',$7)",
        )
        .bind(&task_id)
        .bind(session.0)
        .bind(&session.1)
        .bind(&task_name)
        .bind(&session.2)
        .bind(&authorized_scope)
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
        .bind(&authorized_scope)
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
        // Release session and close active run so the frontend reflects correct state
        sqlx::query(
            "WITH closed AS (
                UPDATE pent_runs SET status='completed', completed_at=now()
                WHERE id=$2 AND status='running'
             )
             UPDATE pent_agent_sessions SET status='ready', active_run_id=NULL, updated_at=now()
             WHERE id=$1 AND status='running'",
        )
        .bind(&result.session_id)
        .bind(&result.run_id)
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
        let mut tx = self.pool.begin().await?;
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
        .execute(&mut *tx)
        .await?;
        if retryable {
            sqlx::query(
                "UPDATE pent_agent_sessions s SET status='ready',updated_at=now()
                 FROM pent_conversation_turns t
                 WHERE t.id=$1 AND s.id=t.session_id",
            )
            .bind(turn_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "WITH s AS (
                    SELECT s2.id AS session_id, s2.active_run_id
                    FROM pent_conversation_turns t
                    JOIN pent_agent_sessions s2 ON s2.id=t.session_id
                    WHERE t.id=$1
                 ), failed AS (
                    UPDATE pent_runs SET status='failed',completed_at=now(),
                        failure_code=$2,failure_message=$3
                    FROM s WHERE pent_runs.id=s.active_run_id
                      AND pent_runs.status IN ('queued','running')
                 )
                 UPDATE pent_agent_sessions SET status='ready',active_run_id=NULL,updated_at=now()
                 FROM s WHERE pent_agent_sessions.id=s.session_id",
            )
            .bind(turn_id)
            .bind(code)
            .bind(message)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
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
        let (outcome, permission_decision, error_code, retryable) =
            classify_persisted_tool_result(success, content_value.as_ref());
        let fallback_backend = if content_value
            .as_ref()
            .and_then(|value| value.get("blocked"))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            "policy"
        } else if matches!(tool_name, "js_analyze" | "knowledge_search") {
            "builtin"
        } else {
            "host"
        };
        let executor_backend = content_value
            .as_ref()
            .and_then(|value| value.get("execution_audit"))
            .and_then(|value| value.get("backend"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(fallback_backend)
            .to_owned();
        let hash = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
        let mut execution_audit = content_value
            .as_ref()
            .and_then(|value| value.get("execution_audit"))
            .cloned()
            .unwrap_or_else(|| {
                serde_json::json!({
                    "backend": &executor_backend,
                    "successful_observation": success,
                    "semantic_fingerprint": super::scan_progress::semantic_result_fingerprint(
                        tool_name,
                        content
                    )
                })
            });
        if let Some(audit) = execution_audit.as_object_mut() {
            audit.insert("outcome".into(), outcome.into());
            audit.insert("retryable".into(), retryable.into());
            if let Some(error_code) = error_code {
                audit.insert("error_code".into(), error_code.into());
            }
            audit.insert("successful_observation".into(), success.into());
            audit.insert(
                "semantic_fingerprint".into(),
                super::scan_progress::semantic_result_fingerprint(tool_name, content).into(),
            );
        }
        let exit_code = content_value
            .as_ref()
            .and_then(|value| value.get("exit_code"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(if success { 0 } else { 1 });
        let network_policy = execution_audit
            .get("network_policy")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pent_tool_calls(
                 id,run_id,tool_name,params_summary,permission_decision,
                 started_at,completed_at,duration_ms,exit_code,executor_backend,
                 network_policy,execution_audit
             ) VALUES($1,$2,$3,$4,$5,now(),now(),$6,$7,$8,$9,$10)
             ON CONFLICT(id) DO UPDATE SET
                 completed_at=now(),duration_ms=excluded.duration_ms,
                 exit_code=excluded.exit_code,network_policy=excluded.network_policy,
                 execution_audit=excluded.execution_audit",
        )
        .bind(tool_call_id)
        .bind(run_id)
        .bind(tool_name)
        .bind(redact_arguments(arguments.clone()))
        .bind(permission_decision)
        .bind(i64::try_from(duration_ms).unwrap_or(i64::MAX))
        .bind(exit_code)
        .bind(&executor_backend)
        .bind(network_policy)
        .bind(execution_audit)
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

    async fn begin_background_tool_execution(
        &self,
        run_id: &str,
        execution_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<()> {
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .map_err(|error| anyhow::anyhow!("后台工具参数不是有效 JSON: {error}"))?;
        sqlx::query(
            "INSERT INTO pent_tool_calls(
                 id,run_id,tool_name,params_summary,permission_decision,
                 started_at,executor_backend,execution_audit
             ) VALUES($1,$2,$3,$4,'allowed',now(),'worker',$5)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(execution_id)
        .bind(run_id)
        .bind(tool_name)
        .bind(redact_arguments(arguments))
        .bind(serde_json::json!({
            "outcome": "running",
            "detached": true,
            "successful_observation": false
        }))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn cancel_background_tool_execution(
        &self,
        run_id: &str,
        execution_id: &str,
        reason: &str,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE pent_tool_calls SET
                 completed_at=now(), exit_code=130,
                 execution_audit=execution_audit || $3::jsonb
             WHERE run_id=$1 AND id=$2 AND completed_at IS NULL",
        )
        .bind(run_id)
        .bind(execution_id)
        .bind(serde_json::json!({
            "outcome": "cancelled",
            "error_code": "TOOL_CANCELLED",
            "cancel_reason": reason,
            "successful_observation": false
        }))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn pending_background_tool_executions(
        &self,
        run_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT id FROM pent_tool_calls
             WHERE run_id=$1 AND completed_at IS NULL
               AND execution_audit->>'detached'='true'
             ORDER BY started_at LIMIT 50",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn load_run_tool_history(&self, run_id: &str) -> anyhow::Result<RunToolHistory> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                serde_json::Value,
                Option<i32>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            "SELECT tc.tool_name, tc.params_summary, tc.exit_code,
                    e.id, tc.execution_audit->>'semantic_fingerprint', e.content_hash
             FROM pent_tool_calls tc
             LEFT JOIN pent_evidence e ON e.tool_call_id=tc.id AND e.run_id=tc.run_id
             WHERE tc.run_id=$1
             ORDER BY tc.started_at DESC
             LIMIT $2",
        )
        .bind(run_id)
        .bind(MAX_HISTORY_TOOL_CALLS)
        .fetch_all(&self.pool)
        .await?;

        let mut evidence_ids = Vec::new();
        let mut tool_calls = Vec::with_capacity(rows.len());
        for (tool_name, arguments, exit_code, evidence_id, semantic_fingerprint, content_hash) in
            rows.into_iter().rev()
        {
            if let Some(evidence_id) = evidence_id {
                evidence_ids.push(evidence_id);
            }
            tool_calls.push(HistoricalToolCall {
                semantic_fingerprint: semantic_fingerprint
                    .or_else(|| content_hash.map(|hash| format!("{tool_name}:{hash}"))),
                tool_name,
                arguments,
                success: exit_code == Some(0),
            });
        }
        let finding_targets = sqlx::query_scalar::<_, String>(
            "SELECT target FROM pent_findings WHERE run_id=$1 ORDER BY created_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(RunToolHistory {
            evidence_ids,
            tool_calls,
            finding_targets,
        })
    }

    async fn execute_evidence_tool(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .map_err(|error| anyhow::anyhow!("Evidence 工具参数不是有效 JSON: {error}"))?;
        match tool_name {
            "evidence_list" => {
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(20)
                    .clamp(1, MAX_EVIDENCE_PAGE_SIZE);
                let offset = arguments
                    .get("offset")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                let rows = sqlx::query_as::<_, (String, String, String, String, i64, String)>(
                    "SELECT e.id, tc.tool_name, e.content_hash, e.content_type,
                            octet_length(e.source)::bigint, e.created_at::text
                     FROM pent_evidence e
                     JOIN pent_tool_calls tc ON tc.id=e.tool_call_id AND tc.run_id=e.run_id
                     WHERE e.run_id=$1
                     ORDER BY e.created_at DESC
                     LIMIT $2 OFFSET $3",
                )
                .bind(run_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
                Ok(serde_json::json!({
                    "items": rows.into_iter().map(|row| serde_json::json!({
                        "evidence_id": row.0,
                        "tool_name": row.1,
                        "content_hash": row.2,
                        "content_type": row.3,
                        "size_bytes": row.4,
                        "created_at": row.5,
                    })).collect::<Vec<_>>(),
                    "limit": limit,
                    "offset": offset,
                    "next_offset": offset + limit,
                }))
            }
            "evidence_search" => {
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|query| !query.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("evidence_search 缺少非空 query"))?;
                let evidence_id = arguments
                    .get("evidence_id")
                    .and_then(serde_json::Value::as_str);
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(10)
                    .clamp(1, 20);
                let rows = sqlx::query_as::<_, (String, String, String)>(
                    "SELECT e.id, tc.tool_name, e.source
                     FROM pent_evidence e
                     JOIN pent_tool_calls tc ON tc.id=e.tool_call_id AND tc.run_id=e.run_id
                     WHERE e.run_id=$1
                       AND ($2::text IS NULL OR e.id=$2)
                       AND strpos(lower(e.source), lower($3)) > 0
                     ORDER BY e.created_at DESC
                     LIMIT $4",
                )
                .bind(run_id)
                .bind(evidence_id)
                .bind(query)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
                Ok(serde_json::json!({
                    "query": query,
                    "items": rows.into_iter().map(|(id, tool_name, source)| {
                        let redacted = sanitize_evidence_text(&source);
                        let (snippet, match_offset) = evidence_search_snippet(&redacted, query, 800);
                        serde_json::json!({
                            "evidence_id": id,
                            "tool_name": tool_name,
                            "match_offset": match_offset,
                            "snippet": snippet,
                            "redacted": true,
                        })
                    }).collect::<Vec<_>>(),
                }))
            }
            "evidence_view" => {
                let evidence_id = arguments
                    .get("evidence_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("evidence_view 缺少 evidence_id"))?;
                let offset = arguments
                    .get("offset")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4_000)
                    .clamp(1, MAX_EVIDENCE_VIEW_CHARS as u64) as usize;
                let row = sqlx::query_as::<_, (String, String, String, String)>(
                    "SELECT tc.tool_name, e.content_hash, e.content_type, e.source
                     FROM pent_evidence e
                     JOIN pent_tool_calls tc ON tc.id=e.tool_call_id AND tc.run_id=e.run_id
                     WHERE e.run_id=$1 AND e.id=$2",
                )
                .bind(run_id)
                .bind(evidence_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("当前 Run 中不存在 Evidence {evidence_id}"))?;
                let redacted = sanitize_evidence_text(&row.3);
                let total_chars = redacted.chars().count();
                let content = redacted
                    .chars()
                    .skip(offset)
                    .take(limit)
                    .collect::<String>();
                let next_offset = offset.saturating_add(content.chars().count());
                Ok(serde_json::json!({
                    "evidence_id": evidence_id,
                    "tool_name": row.0,
                    "content_hash": row.1,
                    "content_type": row.2,
                    "content": content,
                    "offset": offset,
                    "next_offset": next_offset,
                    "total_chars": total_chars,
                    "has_more": next_offset < total_chars,
                    "redacted": true,
                }))
            }
            "query_execution_result" | "query-execution-result" => {
                let execution_id = arguments
                    .get("execution_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty() && value.len() <= 100)
                    .ok_or_else(|| {
                        anyhow::anyhow!("query_execution_result 缺少有效 execution_id")
                    })?;
                let page = arguments
                    .get("page")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1)
                    .max(1) as usize;
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(100)
                    .clamp(1, 500) as usize;
                let search = arguments
                    .get("search")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let filter = arguments
                    .get("filter")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let row = sqlx::query_as::<
                    _,
                    (
                        String,
                        Option<i32>,
                        Option<String>,
                        Option<i64>,
                        Option<String>,
                        Option<String>,
                        serde_json::Value,
                        Option<String>,
                    ),
                >(
                    "SELECT tc.tool_name, tc.exit_code, tc.completed_at::text,
                            tc.duration_ms, e.id, e.source, tc.params_summary,
                            tc.execution_audit->>'outcome'
                     FROM pent_tool_calls tc
                     LEFT JOIN pent_evidence e
                       ON e.tool_call_id=tc.id AND e.run_id=tc.run_id
                     WHERE tc.run_id=$1 AND tc.id=$2
                     ORDER BY e.created_at DESC NULLS LAST
                     LIMIT 1",
                )
                .bind(run_id)
                .bind(execution_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("当前 Run 中不存在 execution {execution_id}"))?;
                let content = sanitize_evidence_text(row.5.as_deref().unwrap_or_default());
                let mut lines = content.lines().collect::<Vec<_>>();
                if let Some(value) = filter {
                    lines.retain(|line| line.contains(value));
                }
                if let Some(value) = search {
                    lines.retain(|line| line.contains(value));
                }
                let total_lines = lines.len();
                let total_pages = total_lines.div_ceil(limit);
                let start = page.saturating_sub(1).saturating_mul(limit);
                let items = lines
                    .into_iter()
                    .skip(start)
                    .take(limit)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({
                    "execution_id": execution_id,
                    "tool_name": row.0,
                    "status": match (row.2.is_none(), row.7.as_deref(), row.1) {
                        (true, _, _) => "running",
                        (_, Some("cancelled"), _) => "cancelled",
                        (_, Some("timed_out"), _) => "timed_out",
                        (_, _, Some(0)) => "completed",
                        _ => "failed",
                    },
                    "exit_code": row.1,
                    "completed_at": row.2,
                    "duration_ms": row.3,
                    "evidence_id": row.4,
                    "arguments": row.6,
                    "outcome": row.7,
                    "page": page,
                    "limit": limit,
                    "total_lines": total_lines,
                    "total_pages": total_pages,
                    "lines": items,
                    "redacted": true,
                }))
            }
            _ => anyhow::bail!("未知 Evidence 工具: {tool_name}"),
        }
    }

    async fn evidence_texts(
        &self,
        run_id: &str,
        evidence_ids: &[String],
    ) -> anyhow::Result<Vec<(String, String)>> {
        if evidence_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT id, source FROM pent_evidence WHERE run_id=$1 AND id = ANY($2)",
        )
        .bind(run_id)
        .bind(evidence_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn record_http_traffic(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
        content: &str,
        replay_of: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .unwrap_or_else(|_| serde_json::json!({}));
        let drafts = super::traffic::drafts_from_tool_result(tool_name, &arguments, content);
        let mut ids = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let id = uuid::Uuid::new_v4().to_string();
            let sha256 = draft
                .response_body
                .as_ref()
                .map(|body| format!("{:x}", Sha256::digest(body.as_bytes())));
            sqlx::query(
                "INSERT INTO pent_http_traffic(
                     id, run_id, tool_call_id, tool_name, method, url,
                     request_headers, request_body, response_status, response_headers,
                     response_body, response_sha256, duration_ms, replay_of, created_at
                 ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now())",
            )
            .bind(&id)
            .bind(run_id)
            .bind(tool_call_id)
            .bind(tool_name)
            .bind(&draft.method)
            .bind(&draft.url)
            .bind(&draft.request_headers)
            .bind(&draft.request_body)
            .bind(draft.response_status)
            .bind(&draft.response_headers)
            .bind(&draft.response_body)
            .bind(sha256)
            .bind(draft.duration_ms)
            .bind(replay_of)
            .execute(&self.pool)
            .await?;
            ids.push(id);
        }
        Ok(ids)
    }

    async fn execute_traffic_tool(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .map_err(|error| anyhow::anyhow!("流量工具参数不是有效 JSON: {error}"))?;
        match tool_name {
            "traffic_list" => {
                let url_contains = arguments
                    .get("url_contains")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let method = arguments
                    .get("method")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let status = arguments
                    .get("status")
                    .and_then(serde_json::Value::as_i64)
                    .map(|value| i32::try_from(value).unwrap_or(i32::MAX));
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(20)
                    .clamp(1, 50);
                let offset = arguments
                    .get("offset")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .max(0);
                let rows = sqlx::query_as::<
                    _,
                    (
                        String,
                        String,
                        String,
                        String,
                        Option<i32>,
                        Option<i64>,
                        Option<String>,
                        String,
                    ),
                >(
                    "SELECT id, tool_name, method, url, response_status, duration_ms,
                            replay_of, created_at::text
                     FROM pent_http_traffic
                     WHERE run_id=$1
                       AND ($2::text IS NULL OR url ILIKE '%'||$2||'%')
                       AND ($3::text IS NULL OR method=upper($3))
                       AND ($4::int IS NULL OR response_status=$4)
                     ORDER BY created_at DESC
                     LIMIT $5 OFFSET $6",
                )
                .bind(run_id)
                .bind(url_contains)
                .bind(method)
                .bind(status)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?;
                Ok(serde_json::json!({
                    "items": rows.into_iter().map(|row| serde_json::json!({
                        "traffic_id": row.0,
                        "tool_name": row.1,
                        "method": row.2,
                        "url": row.3,
                        "response_status": row.4,
                        "duration_ms": row.5,
                        "replay_of": row.6,
                        "created_at": row.7,
                    })).collect::<Vec<_>>(),
                    "limit": limit,
                    "offset": offset,
                    "next_offset": offset + limit,
                }))
            }
            "traffic_sitemap" => {
                let host_filter = arguments
                    .get("host_filter")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty());
                let status_min = arguments
                    .get("status_min")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(200);
                let min_requests = arguments
                    .get("min_requests")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(1)
                    .max(1);
                // Aggregate by host+path: group unique endpoints with method/status summary
                let rows = sqlx::query_as::<_, (String, String, Vec<String>, Vec<i32>, i64)>(
                    "SELECT
                        split_part(url, '/', 3) AS host,
                        '/' || ltrim(
                            substring(url from position('/' in substring(url from position('//' in url) + 2))),
                            '/'
                        ) AS path,
                        array_agg(DISTINCT method ORDER BY method) AS methods,
                        array_agg(DISTINCT response_status ORDER BY response_status)
                            FILTER (WHERE response_status IS NOT NULL) AS status_codes,
                        count(*)::bigint AS request_count
                     FROM pent_http_traffic
                     WHERE run_id = $1
                       AND ($2::text IS NULL OR url ILIKE '%' || $2 || '%')
                       AND (response_status IS NULL OR response_status >= $3)
                     GROUP BY host, path
                     HAVING count(*) >= $4
                     ORDER BY host, path",
                )
                .bind(run_id)
                .bind(host_filter)
                .bind(status_min as i32)
                .bind(min_requests)
                .fetch_all(&self.pool)
                .await?;
                // Organize by host
                let mut hosts: std::collections::BTreeMap<String, Vec<serde_json::Value>> =
                    std::collections::BTreeMap::new();
                for (host, path, methods, status_codes, count) in rows {
                    hosts.entry(host).or_default().push(serde_json::json!({
                        "path": path,
                        "methods": methods,
                        "status_codes": status_codes,
                        "request_count": count,
                    }));
                }
                let summary: Vec<serde_json::Value> = hosts
                    .into_iter()
                    .map(|(host, paths)| {
                        let total: i64 = paths
                            .iter()
                            .filter_map(|p| p["request_count"].as_i64())
                            .sum();
                        serde_json::json!({
                            "host": host,
                            "total_endpoints": paths.len(),
                            "total_requests": total,
                            "endpoints": paths,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({
                    "hosts": summary,
                    "total_hosts": summary.len(),
                    "instruction": "sitemap 聚合了已存储 HTTP 流量的 host+path 视图。端点路径由请求 URL 自动提取，仅供参考。使用 traffic_view 查看具体请求/响应详情。"
                }))
            }
            "traffic_view" => {
                let traffic_id = arguments
                    .get("traffic_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("traffic_view 缺少 traffic_id"))?;
                let offset = arguments
                    .get("offset")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4_000)
                    .clamp(1, MAX_EVIDENCE_VIEW_CHARS as u64) as usize;
                let row = sqlx::query_as::<
                    _,
                    (
                        String,
                        String,
                        String,
                        serde_json::Value,
                        Option<String>,
                        Option<i32>,
                        Option<serde_json::Value>,
                        Option<String>,
                    ),
                >(
                    "SELECT tool_name, method, url, request_headers, request_body,
                            response_status, response_headers, response_body
                     FROM pent_http_traffic
                     WHERE run_id=$1 AND id=$2",
                )
                .bind(run_id)
                .bind(traffic_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("当前 Run 中不存在流量记录 {traffic_id}"))?;
                let (
                    tool,
                    method,
                    url,
                    request_headers,
                    request_body,
                    status,
                    response_headers,
                    response_body,
                ) = row;
                let body = response_body.unwrap_or_default();
                let redacted = sanitize_evidence_text(&body);
                let total_chars = redacted.chars().count();
                let content = redacted
                    .chars()
                    .skip(offset)
                    .take(limit)
                    .collect::<String>();
                let next_offset = offset.saturating_add(content.chars().count());
                Ok(serde_json::json!({
                    "traffic_id": traffic_id,
                    "tool_name": tool,
                    "request": {
                        "method": method,
                        "url": url,
                        "headers": redact_sensitive_value(request_headers),
                        "body": request_body.map(|body| sanitize_evidence_text(&body)),
                    },
                    "response": {
                        "status": status,
                        "headers": response_headers
                            .map(redact_sensitive_value)
                            .unwrap_or_else(|| serde_json::json!({})),
                        "body": content,
                        "offset": offset,
                        "next_offset": next_offset,
                        "total_chars": total_chars,
                        "has_more": next_offset < total_chars,
                    },
                    "redacted": true,
                }))
            }
            "traffic_diff" => {
                let traffic_id_a = arguments
                    .get("traffic_id_a")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("traffic_diff 缺少 traffic_id_a"))?;
                let traffic_id_b = arguments
                    .get("traffic_id_b")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("traffic_diff 缺少 traffic_id_b"))?;
                let rows = sqlx::query_as::<_, (String, Option<i32>, Option<String>)>(
                    "SELECT id, response_status, response_body
                     FROM pent_http_traffic
                     WHERE run_id=$1 AND id IN ($2,$3)",
                )
                .bind(run_id)
                .bind(traffic_id_a)
                .bind(traffic_id_b)
                .fetch_all(&self.pool)
                .await?;
                let find = |id: &str| {
                    rows.iter()
                        .find(|row| row.0 == id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("当前 Run 中不存在流量记录 {id}"))
                };
                let (_, status_a, body_a) = find(traffic_id_a)?;
                let (_, status_b, body_b) = find(traffic_id_b)?;
                let body_a = body_a.unwrap_or_default();
                let body_b = body_b.unwrap_or_default();
                let mut diff = super::traffic::diff_bodies(&body_a, &body_b);
                if let Some(object) = diff.as_object_mut() {
                    object.insert("status_a".into(), serde_json::json!(status_a));
                    object.insert("status_b".into(), serde_json::json!(status_b));
                    object.insert(
                        "status_equal".into(),
                        serde_json::json!(status_a == status_b),
                    );
                }
                Ok(serde_json::json!({
                    "traffic_id_a": traffic_id_a,
                    "traffic_id_b": traffic_id_b,
                    "diff": diff,
                }))
            }
            _ => anyhow::bail!("未知流量工具: {tool_name}"),
        }
    }

    async fn load_traffic_request(
        &self,
        run_id: &str,
        traffic_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let row = sqlx::query_as::<_, (String, String, serde_json::Value, Option<String>)>(
            "SELECT method, url, request_headers, request_body
             FROM pent_http_traffic
             WHERE run_id=$1 AND id=$2",
        )
        .bind(run_id)
        .bind(traffic_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("当前 Run 中不存在流量记录 {traffic_id}"))?;
        Ok(serde_json::json!({
            "method": row.0,
            "url": row.1,
            "headers": row.2,
            "body": row.3.unwrap_or_default(),
        }))
    }

    async fn record_run_event(
        &self,
        run_id: &str,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 734291))")
            .bind(run_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO pent_run_events(event_id, run_id, sequence, event_type, payload)
             VALUES($1, $2, COALESCE((SELECT max(sequence)+1 FROM pent_run_events WHERE run_id=$2), 1), $3, $4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(run_id)
        .bind(event_type)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
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

fn sanitize_evidence_text(text: &str) -> String {
    let mut value = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => redact_sensitive_value(value).to_string(),
        Err(_) => text.to_string(),
    };
    for key in [
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "secret",
    ] {
        let lower = value.to_ascii_lowercase();
        let mut search_from = 0;
        while let Some(relative) = lower[search_from..].find(key) {
            let start = search_from + relative;
            let end = value[start..]
                .find(['\n', '\r', ',', '}'])
                .map(|length| start + length)
                .unwrap_or(value.len());
            if end > start && end - start < 2_000 {
                value.replace_range(start..end, &format!("{key}=***REDACTED***"));
                break;
            }
            search_from = start.saturating_add(key.len());
        }
    }
    value
}

fn evidence_search_snippet(text: &str, query: &str, radius: usize) -> (String, usize) {
    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();
    let byte_offset = lower_text.find(&lower_query).unwrap_or(0);
    let char_offset = text[..byte_offset.min(text.len())].chars().count();
    let start = char_offset.saturating_sub(radius / 2);
    (text.chars().skip(start).take(radius).collect(), char_offset)
}

fn redact_arguments(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(object) = value.as_object_mut() {
        for (key, item) in object {
            if matches!(
                key.to_ascii_lowercase().as_str(),
                "authorization"
                    | "cookie"
                    | "password"
                    | "token"
                    | "csrf_token"
                    | "_credential_ref"
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
                        | "_credential_ref"
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

#[cfg(test)]
mod classification_tests {
    use super::classify_persisted_tool_result;

    #[test]
    fn distinguishes_all_terminal_tool_outcomes() {
        let cases = [
            (true, serde_json::json!({}), "succeeded", "allowed"),
            (
                false,
                serde_json::json!({"blocked":true}),
                "blocked",
                "denied",
            ),
            (
                false,
                serde_json::json!({"error_code":"TOOL_CANCELLED"}),
                "cancelled",
                "allowed",
            ),
            (
                false,
                serde_json::json!({"error_code":"TOOL_TIMEOUT"}),
                "timed_out",
                "allowed",
            ),
            (
                false,
                serde_json::json!({"error_code":"WORKER_UNAVAILABLE"}),
                "worker_unavailable",
                "allowed",
            ),
            (
                false,
                serde_json::json!({"error_code":"TOOL_EXECUTION_FAILED"}),
                "failed",
                "allowed",
            ),
        ];
        for (success, content, expected_outcome, expected_permission) in cases {
            let (outcome, permission, _, _) =
                classify_persisted_tool_result(success, Some(&content));
            assert_eq!(outcome, expected_outcome);
            assert_eq!(permission, expected_permission);
        }
    }
}

#[cfg(test)]
mod traffic_tests {
    use super::*;

    async fn test_pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database")
    }

    #[tokio::test]
    async fn records_and_queries_http_traffic() {
        if std::env::var("TEST_DATABASE_URL").is_err() {
            return;
        }
        let pool = test_pool().await;
        let repository = PgConversationRepository::new(pool.clone());
        let run_id = "traffic-it-run";

        sqlx::query("DELETE FROM pent_http_traffic WHERE run_id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup traffic");
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup run");
        sqlx::query(
            "INSERT INTO pent_runs(id, task_id, tenant_id, user_id, status, objective_snapshot)
             VALUES ($1,'traffic-it-task',0,'it-tester','running','流量测试')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed run");

        let arguments = serde_json::json!({
            "headers": {"Authorization": "Bearer secret-token", "X-Req": "v"},
            "body": "user=admin"
        })
        .to_string();
        let content = serde_json::json!({
            "url": "http://192.0.2.10/login",
            "method": "POST",
            "status": 200,
            "raw_response": "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html>login failed</html>",
            "elapsed_ms": 42
        })
        .to_string();
        let ids = repository
            .record_http_traffic(run_id, "tc-1", "http_request", &arguments, &content, None)
            .await
            .expect("record traffic");
        assert_eq!(ids.len(), 1);

        let listed = repository
            .execute_traffic_tool(run_id, "traffic_list", "{}")
            .await
            .expect("traffic_list");
        let items = listed["items"].as_array().expect("items array");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["method"], "POST");
        assert_eq!(items[0]["response_status"], 200);

        let filtered = repository
            .execute_traffic_tool(run_id, "traffic_list", r#"{"status": 404}"#)
            .await
            .expect("traffic_list filtered");
        assert_eq!(filtered["items"].as_array().unwrap().len(), 0);

        let view = repository
            .execute_traffic_tool(
                run_id,
                "traffic_view",
                &serde_json::json!({"traffic_id": ids[0]}).to_string(),
            )
            .await
            .expect("traffic_view");
        assert_eq!(view["request"]["method"], "POST");
        assert_eq!(
            view["request"]["headers"]["Authorization"], "[REDACTED]",
            "请求头必须脱敏"
        );
        assert_eq!(view["response"]["status"], 200);
        assert!(
            view["response"]["body"]
                .as_str()
                .unwrap()
                .contains("login failed")
        );

        // Second exchange with a different body so the diff has content.
        let content_b = serde_json::json!({
            "url": "http://192.0.2.10/login",
            "method": "POST",
            "status": 200,
            "raw_response": "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html>welcome admin</html>",
            "elapsed_ms": 51
        })
        .to_string();
        let ids_b = repository
            .record_http_traffic(
                run_id,
                "tc-2",
                "http_request",
                &arguments,
                &content_b,
                Some(&ids[0]),
            )
            .await
            .expect("record replay traffic");
        let diff = repository
            .execute_traffic_tool(
                run_id,
                "traffic_diff",
                &serde_json::json!({"traffic_id_a": ids[0], "traffic_id_b": ids_b[0]}).to_string(),
            )
            .await
            .expect("traffic_diff");
        assert_eq!(diff["diff"]["equal"], false);
        assert_eq!(diff["diff"]["status_equal"], true);
        assert!(diff["diff"]["first_diff"]["line"].as_i64().unwrap() >= 1);

        let replay_args = repository
            .load_traffic_request(run_id, &ids[0])
            .await
            .expect("load_traffic_request");
        assert_eq!(replay_args["method"], "POST");
        assert_eq!(replay_args["url"], "http://192.0.2.10/login");
        assert_eq!(replay_args["body"], "user=admin");

        // evidence_texts only returns rows that actually belong to the run.
        sqlx::query(
            "INSERT INTO pent_evidence(id, run_id, source, content_hash)
             VALUES ('traffic-it-ev1', $1, 'raw tool output with marker', 'hash1')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed evidence");
        let texts = repository
            .evidence_texts(
                run_id,
                &["traffic-it-ev1".to_string(), "missing-id".to_string()],
            )
            .await
            .expect("evidence_texts");
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].0, "traffic-it-ev1");
        assert!(texts[0].1.contains("marker"));
        sqlx::query("DELETE FROM pent_evidence WHERE run_id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup evidence");

        // record_tool_result persists a semantic fingerprint that the history
        // loader returns for cross-turn duplicate suppression.
        repository
            .record_tool_result(
                run_id,
                "tc-3",
                "http_request",
                r#"{"url":"http://192.0.2.10/"}"#,
                true,
                r#"{"status":200,"body":"nonce 0123456789abcdef01234567 page"}"#,
                12,
            )
            .await
            .expect("record_tool_result");
        let history = repository
            .load_run_tool_history(run_id)
            .await
            .expect("load_run_tool_history");
        let recorded = history
            .tool_calls
            .iter()
            .find(|call| call.tool_name == "http_request")
            .expect("history contains the call");
        let fingerprint = recorded
            .semantic_fingerprint
            .as_deref()
            .expect("semantic fingerprint persisted");
        assert!(
            fingerprint.starts_with("http_request:semantic:"),
            "指纹必须是语义形式，实际：{fingerprint}"
        );
        sqlx::query("DELETE FROM pent_tool_calls WHERE run_id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup tool calls");
        // record_tool_result also writes a conversation-memory knowledge row;
        // remove it so snapshot exports stay clean.
        sqlx::query("DELETE FROM pent_knowledge_documents WHERE source='conversation-memory'")
            .execute(&pool)
            .await
            .expect("cleanup conversation memory");

        sqlx::query("DELETE FROM pent_http_traffic WHERE run_id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup traffic after test");
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup run after test");
    }
}

/// Build a human-readable task name from the session objective and scope.
fn task_display_name(objective: &str, scope: &serde_json::Value) -> String {
    let first_target = scope
        .get("targets")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let display = if !first_target.is_empty() && objective != first_target {
        format!("{objective} → {first_target}")
    } else {
        objective.to_string()
    };
    display.chars().take(200).collect()
}

#[cfg(test)]
mod task_name_tests {
    use super::task_display_name;

    #[test]
    fn long_multibyte_objective_fits_task_name_column() {
        let objective = "完整渗透测试".repeat(80);
        let name = task_display_name(
            &objective,
            &serde_json::json!({"targets": ["http://127.0.0.1:18080"]}),
        );
        assert_eq!(name.chars().count(), 200);
        assert!(name.is_char_boundary(name.len()));
    }
}
