use kairos_platform_ai_api::{ChatMessage, ChatRequest};
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationMessageType};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Execute a session-backed Run through the same autonomous conversation
/// runtime used by the UI. Returns `false` for legacy task-only runs.
pub(crate) async fn execute_session_run(
    pool: &PgPool,
    run_id: &str,
    worker_id: &str,
) -> Result<bool, String> {
    let session: Option<(String, Option<String>)> = sqlx::query_as(
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
    let objective = objective
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| format!("安全测试运行 {run_id}"));

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

            // 提取对话中已确认的漏洞并落库到 pent_findings
            match extract_and_persist_findings(pool, run_id, &session_id).await {
                Ok(count) if count > 0 => {
                    append_event(
                        pool,
                        run_id,
                        "run.findings.persisted",
                        json!({"count": count}),
                    )
                    .await?;
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(run_id = %run_id, error = %error, "vulnerability extraction failed");
                }
            }

            match crate::report::generate_run_report(pool, run_id, Some(&result.message.content))
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

/// A vulnerability extracted from the conversation transcript.
struct FindingDraft {
    title: String,
    severity: String,
    target: String,
    description: String,
    confidence: f32,
}

fn finding_fingerprint(title: &str, target: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{}|{}", title.trim(), target.trim()).as_bytes())
    )
}

async fn persist_finding(
    pool: &PgPool,
    run_id: &str,
    finding: &FindingDraft,
    evidence_ids: &[String],
) -> Result<bool, String> {
    if evidence_ids.is_empty() {
        return Err("漏洞记录必须至少引用一条 Evidence".to_string());
    }

    let evidence_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pent_evidence WHERE run_id=$1 AND id = ANY($2)")
            .bind(run_id)
            .bind(evidence_ids)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
    if evidence_count != evidence_ids.len() as i64 {
        return Err("存在不属于当前 Run 或不存在的 Evidence".to_string());
    }

    let result = sqlx::query(
        "INSERT INTO pent_findings(
             id, run_id, project_id, title, severity, status, target,
             evidence_ids, description, fingerprint, confidence, false_positive_status
         ) VALUES(
             $1, $2, (SELECT project_id FROM pent_runs WHERE id=$2), $3, $4,
             'open', $5, $6, $7, $8, $9, 'unreviewed'
         )
         ON CONFLICT (run_id, fingerprint) DO NOTHING",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(&finding.title)
    .bind(&finding.severity)
    .bind(&finding.target)
    .bind(json!(evidence_ids))
    .bind(&finding.description)
    .bind(finding_fingerprint(&finding.title, &finding.target))
    .bind(finding.confidence)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(result.rows_affected() == 1)
}

/// Persist a finding explicitly confirmed by the agent during the tool loop.
pub(crate) async fn record_finding(
    pool: &PgPool,
    run_id: &str,
    arguments: &str,
) -> Result<Value, String> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|error| format!("record_finding 参数不是有效 JSON: {error}"))?;
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if title.is_empty() {
        return Err("title 不能为空".to_string());
    }
    if title.chars().count() > 500 {
        return Err("title 不能超过 500 个字符".to_string());
    }
    let severity = value
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        severity.as_str(),
        "critical" | "high" | "medium" | "low" | "info"
    ) {
        return Err("severity 必须是 critical/high/medium/low/info".to_string());
    }
    let evidence_ids = value
        .get("evidence_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "evidence_ids 必须是非空字符串数组".to_string())?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if evidence_ids.is_empty() {
        return Err("evidence_ids 必须是非空字符串数组".to_string());
    }

    let finding = FindingDraft {
        title: title.to_string(),
        severity,
        target: value
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        confidence: value
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0) as f32,
    };
    let fingerprint = finding_fingerprint(&finding.title, &finding.target);
    let inserted = persist_finding(pool, run_id, &finding, &evidence_ids).await?;
    Ok(json!({
        "recorded": inserted,
        "duplicate": !inserted,
        "fingerprint": fingerprint,
        "status": "open",
        "false_positive_status": "unreviewed"
    }))
}

/// Ask the model to summarize confirmed vulnerabilities into a JSON array,
/// then persist them into `pent_findings` (deduplicated by fingerprint).
pub(crate) async fn extract_and_persist_findings(
    pool: &PgPool,
    run_id: &str,
    session_id: &str,
) -> Result<usize, String> {
    let messages: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM pent_conversation_messages
         WHERE session_id=$1 AND role='assistant'
         ORDER BY created_at",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let transcript: String = messages
        .iter()
        .map(|(_, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
        .chars()
        .take(120_000)
        .collect();
    if transcript.trim().is_empty() {
        return Ok(0);
    }

    let model_id: i64 = sqlx::query_scalar(
        "SELECT id FROM ai.model_configs WHERE type='chat' AND status=0 ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "未配置可用的聊天模型".to_string())?;

    let factory = AiModelFactory::new(pool.clone());
    let response = factory
        .chat(
            model_id,
            ChatRequest {
                model: String::new(),
                messages: vec![
                    ChatMessage {
                        role: "system".into(),
                        content: "你是渗透测试漏洞提取器。从下面的渗透测试对话记录中提取所有「已确认」的漏洞，只输出 JSON 数组。每个元素字段：title（漏洞名）、severity（只能是 critical/high/medium/low/info）、target（目标 URL 或端点）、description（一句话证据）、confidence（0~1 浮点数）。没有漏洞时输出 []。只输出 JSON，不要任何解释。".into(),
                    },
                    ChatMessage {
                        role: "user".into(),
                        content: transcript,
                    },
                ],
                temperature: Some(0.0),
                max_tokens: Some(4000),
            },
        )
        .await
        .map_err(|e| format!("漏洞提取模型调用失败: {e:?}"))?;

    let findings = parse_findings_json(&response.content);
    tracing::info!(
        run_id = %run_id,
        count = findings.len(),
        "findings extraction completed"
    );

    // pent_findings 约束要求 evidence_ids 非空；取该 run 的首条 evidence，
    // 无 evidence 时创建一条占位证据。
    let evidence_id: String = match sqlx::query_scalar::<_, String>(
        "SELECT id FROM pent_evidence WHERE run_id=$1 ORDER BY created_at LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO pent_evidence(id, run_id, tool_call_id, source, content_hash, content_type)
                 VALUES($1, $2, NULL, '漏洞发现记录', '', 'text/plain')",
            )
            .bind(&id)
            .bind(run_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            id
        }
    };
    let evidence_ids = vec![evidence_id];

    let mut inserted = 0;
    for finding in findings {
        if persist_finding(pool, run_id, &finding, &evidence_ids).await? {
            inserted += 1;
        }
    }
    Ok(inserted)
}

fn parse_findings_json(content: &str) -> Vec<FindingDraft> {
    let mut text = content.trim();
    if text.starts_with("```") {
        text = text.trim_start_matches("```json").trim_start_matches("```");
        if let Some(end) = text.rfind("```") {
            text = &text[..end];
        }
    }
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            // Fallback: extract the first JSON array from the text.
            let (Some(start), Some(end)) = (text.find('['), text.rfind(']')) else {
                return Vec::new();
            };
            if start >= end {
                return Vec::new();
            }
            match serde_json::from_str(&text[start..=end]) {
                Ok(value) => value,
                Err(_) => return Vec::new(),
            }
        }
    };
    let array = value.as_array().cloned().unwrap_or_default();
    array
        .into_iter()
        .filter_map(|item| {
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if title.is_empty() {
                return None;
            }
            let severity = item
                .get("severity")
                .and_then(Value::as_str)
                .map(|s| s.to_ascii_lowercase())
                .filter(|s| matches!(s.as_str(), "critical" | "high" | "medium" | "low" | "info"))
                .unwrap_or_else(|| "info".to_string());
            Some(FindingDraft {
                title,
                severity,
                target: item
                    .get("target")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                confidence: item
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0) as f32,
            })
        })
        .collect()
}
