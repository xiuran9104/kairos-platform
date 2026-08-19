use kairos_platform_ai_api::{ChatMessage, ChatRequest};
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{AgentTurnRequest, ConversationMessageType};
use kairos_platform_pent_runtime::ports::FindingRecorder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::model_gateway::AiModelGateway;

#[derive(Clone)]
pub struct PgFindingRecorder {
    pool: PgPool,
}

impl PgFindingRecorder {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl FindingRecorder for PgFindingRecorder {
    async fn record(&self, run_id: &str, arguments: &str) -> anyhow::Result<Value> {
        record_finding(&self.pool, run_id, arguments)
            .await
            .map_err(anyhow::Error::msg)
    }

    async fn reconcile(&self, run_id: &str, session_id: &str) -> anyhow::Result<usize> {
        extract_and_persist_findings(&self.pool, run_id, session_id)
            .await
            .map_err(anyhow::Error::msg)
    }
}

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
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 734291))")
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
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
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

/// Server-side minimum length (in characters) for a finding description, so
/// a record stays reproducible on its own instead of being a one-line claim.
const MIN_FINDING_DESCRIPTION_CHARS: usize = 20;

/// A vulnerability extracted from the conversation transcript.
struct FindingDraft {
    title: String,
    severity: String,
    target: String,
    description: String,
    confidence: f32,
}

fn finding_fingerprint(title: &str, target: &str) -> String {
    let semantic_key = dvwa_module_key(target)
        .map(|module| format!("dvwa|{}|{module}", canonical_target_authority(target)))
        .unwrap_or_else(|| {
            format!(
                "{}|{}",
                normalized_vulnerability_kind(title),
                canonical_target_scope(target)
            )
        });
    format!("{:x}", Sha256::digest(semantic_key.as_bytes()))
}

fn dvwa_module_key(target: &str) -> Option<&'static str> {
    let normalized = target.to_ascii_lowercase();
    [
        ("/vulnerabilities/brute/", "brute-force"),
        ("/vulnerabilities/exec/", "command-injection"),
        ("/vulnerabilities/csrf/", "csrf"),
        ("/vulnerabilities/fi/", "file-inclusion"),
        ("/vulnerabilities/upload/", "file-upload"),
        ("/vulnerabilities/captcha/", "insecure-captcha"),
        ("/vulnerabilities/sqli_blind/", "blind-sql-injection"),
        ("/vulnerabilities/sqli/", "sql-injection"),
        ("/vulnerabilities/weak_id/", "weak-session-id"),
        ("/vulnerabilities/xss_d/", "dom-xss"),
        ("/vulnerabilities/xss_r/", "reflected-xss"),
        ("/vulnerabilities/xss_s/", "stored-xss"),
        ("/vulnerabilities/csp/", "csp-bypass"),
        ("/vulnerabilities/javascript/", "javascript"),
    ]
    .into_iter()
    .find_map(|(path, module)| normalized.contains(path).then_some(module))
}

fn canonical_target_authority(target: &str) -> String {
    reqwest::Url::parse(target)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?;
            Some(match url.port() {
                Some(port) => format!("{}://{host}:{port}", url.scheme()),
                None => format!("{}://{host}", url.scheme()),
            })
        })
        .unwrap_or_else(|| target.split('/').take(3).collect::<Vec<_>>().join("/"))
        .to_ascii_lowercase()
}

fn canonical_target_scope(target: &str) -> String {
    if let Ok(mut url) = reqwest::Url::parse(target) {
        url.set_query(None);
        url.set_fragment(None);
        return url.to_string().trim_end_matches('/').to_ascii_lowercase();
    }
    target
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/')
        .trim()
        .to_ascii_lowercase()
}

fn normalized_vulnerability_kind(title: &str) -> String {
    let normalized = title.to_ascii_lowercase();
    let kinds = [
        (
            &["command injection", "os command", "命令注入"][..],
            "command-injection",
        ),
        (&["blind sql", "盲注", "盲 sql"][..], "blind-sql-injection"),
        (&["sql injection", "sql 注入"][..], "sql-injection"),
        (
            &[
                "local file inclusion",
                "lfi",
                "本地文件包含",
                "任意本地文件读取",
            ][..],
            "file-inclusion",
        ),
        (
            &["weak session", "session id", "会话标识符"][..],
            "weak-session-id",
        ),
        (&["stored xss", "存储型 xss"][..], "stored-xss"),
        (&["reflected xss", "反射型 xss"][..], "reflected-xss"),
        (&["dom xss", "dom-xss"][..], "dom-xss"),
        (&["csrf", "跨站请求伪造"][..], "csrf"),
        (&["file upload", "文件上传"][..], "file-upload"),
    ];
    kinds
        .into_iter()
        .find_map(|(aliases, kind)| {
            aliases
                .iter()
                .any(|alias| normalized.contains(alias))
                .then_some(kind)
        })
        .map(str::to_string)
        .unwrap_or_else(|| normalized.split_whitespace().collect::<Vec<_>>().join("-"))
}

async fn persist_finding(
    pool: &PgPool,
    run_id: &str,
    finding: &FindingDraft,
    evidence_ids: &[String],
) -> Result<bool, String> {
    let issues = finding_quality_issues(finding, evidence_ids);
    if !issues.is_empty() {
        return Err(format!("finding 质量校验失败：{}", issues.join("；")));
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

    let fingerprint = finding_fingerprint(&finding.title, &finding.target);
    // Older runs used an exact title+target fingerprint. Check their semantic
    // identity in application code so deploying the new fingerprint does not
    // add one more duplicate beside legacy rows.
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT title, target FROM pent_findings WHERE run_id=$1",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if existing
        .iter()
        .any(|(title, target)| finding_fingerprint(title, target) == fingerprint)
    {
        return Ok(false);
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
    .bind(fingerprint)
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
    let severity = value
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let evidence_ids = value
        .get("evidence_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
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
    let issues = finding_quality_issues(&finding, &evidence_ids);
    if !issues.is_empty() {
        return Err(format!(
            "record_finding 参数校验失败：{}",
            issues.join("；")
        ));
    }
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

    let model_id = AiModelGateway::new(pool.clone())
        .resolve_model(session_id)
        .await
        .map_err(|error| error.to_string())?
        .0;

    let factory = AiModelFactory::new(pool.clone());
    let response = factory
        .chat(
            model_id,
            ChatRequest {
                model: String::new(),
                messages: vec![
                    ChatMessage {
                        role: "system".into(),
                        content: "你是渗透测试漏洞提取器。从下面的渗透测试对话记录中提取所有「已确认」的漏洞，只输出 JSON 数组。每个元素字段：title（漏洞名）、severity（只能是 critical/high/medium/low/info）、target（必填，具体 URL 或主机:端口）、description（至少 20 字符，必须写清触发点、有效 payload/动作、响应特征与影响，使记录可独立复现）、confidence（0~1 浮点数）。缺少具体目标或复现证据的线索不要输出；没有合格漏洞时输出 []。只输出 JSON，不要任何解释。".into(),
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

fn finding_quality_issues(finding: &FindingDraft, evidence_ids: &[String]) -> Vec<String> {
    let mut issues = Vec::new();
    let title_chars = finding.title.trim().chars().count();
    if title_chars == 0 {
        issues.push("title 不能为空".to_string());
    } else if title_chars > 500 {
        issues.push("title 不能超过 500 个字符".to_string());
    }
    if !matches!(
        finding.severity.as_str(),
        "critical" | "high" | "medium" | "low" | "info"
    ) {
        issues.push("severity 必须是 critical/high/medium/low/info".to_string());
    }
    if finding.target.trim().is_empty() {
        issues.push("target 不能为空，必须是具体 URL 或主机:端口".to_string());
    }
    let description_chars = finding.description.trim().chars().count();
    if description_chars < MIN_FINDING_DESCRIPTION_CHARS {
        issues.push(format!(
            "description 至少需要 {MIN_FINDING_DESCRIPTION_CHARS} 个字符，且应包含触发点、payload/动作和响应特征（当前 {description_chars}）"
        ));
    }
    if evidence_ids.is_empty() {
        issues.push("evidence_ids 必须是非空字符串数组".to_string());
    }
    issues
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
            let finding = FindingDraft {
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
            };
            // Terminal extraction is a safety net, not a license to create
            // incomplete claims. Only pass drafts that satisfy the same
            // target/description contract as explicit record_finding calls.
            finding_quality_issues(&finding, &["terminal-extraction".into()])
                .is_empty()
                .then_some(finding)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dvwa_fingerprint_deduplicates_title_and_query_variants() {
        let first = finding_fingerprint(
            "File Inclusion: /etc/passwd 任意本地文件读取",
            "http://127.0.0.1:18080/vulnerabilities/fi/?page=../../etc/passwd",
        );
        let second = finding_fingerprint(
            "Local File Inclusion (LFI) via URL-encoded page parameter",
            "http://127.0.0.1:18080/vulnerabilities/fi/",
        );
        assert_eq!(first, second);
    }

    #[test]
    fn fingerprints_keep_distinct_dvwa_modules_separate() {
        assert_ne!(
            finding_fingerprint("DVWA finding", "http://127.0.0.1:18080/vulnerabilities/fi/"),
            finding_fingerprint(
                "DVWA finding",
                "http://127.0.0.1:18080/vulnerabilities/exec/"
            )
        );
    }

    fn draft(target: &str, description: &str) -> FindingDraft {
        FindingDraft {
            title: "SQL Injection".into(),
            severity: "high".into(),
            target: target.into(),
            description: description.into(),
            confidence: 1.0,
        }
    }

    #[test]
    fn finding_quality_reports_missing_target_and_short_description_together() {
        let issues = finding_quality_issues(&draft("", "too short"), &[]);
        assert_eq!(issues.len(), 3);
        assert!(issues.iter().any(|issue| issue.contains("target 不能为空")));
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("description 至少"))
        );
        assert!(issues.iter().any(|issue| issue.contains("evidence_ids")));
    }

    #[test]
    fn finding_quality_accepts_reproducible_record() {
        let issues = finding_quality_issues(
            &draft(
                "https://shop.test/api/users?id=1",
                "参数 id 使用 payload 1 OR 1=1 后返回多条用户记录，确认存在 SQL 注入。",
            ),
            &["ev-1".into()],
        );
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn terminal_extraction_drops_incomplete_findings() {
        let findings = parse_findings_json(
            r#"[
                {"title":"SQLi","severity":"high","target":"","description":"confirmed","confidence":1},
                {"title":"XSS","severity":"medium","target":"https://shop.test/search","description":"搜索参数反射 payload <svg/onload=alert(1)>，响应保留事件处理器并可执行。","confidence":0.9}
            ]"#,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].title, "XSS");
    }
}
