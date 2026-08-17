//! Deterministic run report generation — the "fact projection" layer.
//!
//! The report is rendered purely from persisted findings / evidence / tool
//! calls without any LLM involvement, so every statement in it is traceable
//! to a database row (matching the doc requirement that report fields must
//! be deterministically projectable from Evidence). An optional LLM
//! narrative may be appended as a clearly labelled section.

use chrono::{DateTime, Utc};
use kairos_platform_framework_database::PgPool;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::infrastructure::object_store::ObjectStore;

const EVIDENCE_PREVIEW_MAX_CHARS: usize = 1200;
const SNAPSHOT_MAX_CHARS: usize = 2000;

pub struct ReportArtifact {
    pub artifact_id: String,
    pub object_key: String,
    pub size_bytes: i64,
    pub sha256: String,
}

struct RunSnapshot {
    objective: String,
    scope: Value,
    policy: Value,
    budget: Value,
    status: String,
    created_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

struct FindingRow {
    id: String,
    title: String,
    severity: String,
    status: String,
    target: String,
    description: String,
    confidence: f32,
    false_positive_status: String,
    evidence_ids: Vec<String>,
}

struct EvidenceRow {
    id: String,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    /// Full raw source — required for claim projection / maturity scoring.
    /// Only a sanitized, truncated preview is rendered.
    source: String,
    content_hash: String,
    content_type: String,
    created_at: Option<DateTime<Utc>>,
}

struct ToolCallRow {
    tool_name: String,
    executor_backend: String,
    started_at: Option<DateTime<Utc>>,
    duration_ms: Option<i64>,
    exit_code: Option<i32>,
    permission_decision: String,
}

struct HttpTrafficRow {
    tool_call_id: Option<String>,
    method: String,
    url: String,
    request_headers: Value,
    request_body: Option<String>,
    response_status: Option<i32>,
    response_body: Option<String>,
    duration_ms: Option<i64>,
}

/// Supported report output formats.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportFormat {
    Markdown,
    Pdf,
    Docx,
}

impl ReportFormat {
    fn content_type(&self) -> &'static str {
        match self {
            Self::Markdown => "text/markdown; charset=utf-8",
            Self::Pdf => "application/pdf",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
        }
    }
}

/// Render the deterministic Markdown report for a run, store it in MinIO and
/// register it in `pent_artifacts`. Returns the stored artifact metadata.
pub(crate) async fn generate_run_report(
    pool: &PgPool,
    run_id: &str,
    narrative: Option<&str>,
) -> Result<ReportArtifact, String> {
    let run = load_run(pool, run_id).await?;
    let findings = load_findings(pool, run_id).await?;
    let evidence = load_evidence(pool, run_id).await?;
    let tool_calls = load_tool_calls(pool, run_id).await?;
    let http_traffic = load_http_traffic(pool, run_id).await?;

    let markdown = render_markdown(
        run_id,
        &run,
        &findings,
        &evidence,
        &tool_calls,
        &http_traffic,
        narrative,
    );
    let artifact_id = store_artifact(pool, run_id, &markdown, ReportFormat::Markdown).await?;

    // Generate PDF and DOCX artifacts from the same markdown content.
    let pdf_key = format!("pent/runs/{run_id}/report-{artifact_id}.pdf");
    let _ = store_artifact_with_key(
        pool,
        run_id,
        &pdf_key,
        markdown.as_bytes().to_vec(),
        ReportFormat::Pdf,
    )
    .await;
    let docx_key = format!("pent/runs/{run_id}/report-{artifact_id}.docx");
    let _ = store_artifact_with_key(
        pool,
        run_id,
        &docx_key,
        markdown.as_bytes().to_vec(),
        ReportFormat::Docx,
    )
    .await;

    let bytes = markdown.into_bytes();
    let object_key = format!("pent/runs/{run_id}/report-{artifact_id}.md");
    Ok(ReportArtifact {
        artifact_id: artifact_id.clone(),
        object_key,
        size_bytes: bytes.len() as i64,
        sha256: hex::encode(Sha256::digest(&bytes)),
    })
}

async fn store_artifact(
    pool: &PgPool,
    run_id: &str,
    content: &str,
    format: ReportFormat,
) -> Result<String, String> {
    let artifact_id = Uuid::new_v4().to_string();
    let object_key = format!(
        "pent/runs/{run_id}/report-{artifact_id}.{}",
        format.extension()
    );
    store_artifact_with_key(
        pool,
        run_id,
        &object_key,
        content.as_bytes().to_vec(),
        format,
    )
    .await?;
    Ok(artifact_id)
}

async fn store_artifact_with_key(
    pool: &PgPool,
    run_id: &str,
    object_key: &str,
    bytes: Vec<u8>,
    format: ReportFormat,
) -> Result<(), String> {
    let sha256 = hex::encode(Sha256::digest(&bytes));
    ObjectStore::from_env()
        .put_object(object_key, bytes.clone(), format.content_type())
        .await?;
    sqlx::query(
        "INSERT INTO pent_artifacts(id, run_id, object_key, size_bytes, sha256, content_type, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,now())",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(object_key)
    .bind(bytes.len() as i64)
    .bind(&sha256)
    .bind(format.content_type())
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

type RunRowTuple = (
    Option<String>,
    Value,
    Value,
    Value,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
);
type FindingRowTuple = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    f32,
    String,
    Value,
);
type EvidenceRowTuple = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<DateTime<Utc>>,
);
type ToolCallRowTuple = (
    String,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<i64>,
    Option<i32>,
    Option<String>,
);

async fn load_run(pool: &PgPool, run_id: &str) -> Result<RunSnapshot, String> {
    let row: Option<RunRowTuple> = sqlx::query_as(
        "SELECT objective_snapshot, scope_snapshot, policy_snapshot, budget_snapshot,
                status, created_at, started_at, completed_at
         FROM pent_runs WHERE id=$1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let (objective, scope, policy, budget, status, created_at, started_at, completed_at) =
        row.ok_or_else(|| format!("运行 {run_id} 不存在"))?;
    Ok(RunSnapshot {
        objective: objective.unwrap_or_default(),
        scope,
        policy,
        budget,
        status: status.unwrap_or_else(|| "unknown".into()),
        created_at,
        started_at,
        completed_at,
    })
}

async fn load_findings(pool: &PgPool, run_id: &str) -> Result<Vec<FindingRow>, String> {
    let rows: Vec<FindingRowTuple> = sqlx::query_as(
        "SELECT id, title, severity, status, target, description, confidence,
                false_positive_status, evidence_ids
         FROM pent_findings WHERE run_id=$1 ORDER BY created_at",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut findings: Vec<FindingRow> = rows
        .into_iter()
        .map(
            |(id, title, severity, status, target, description, confidence, fp, evidence_ids)| {
                FindingRow {
                    id,
                    title: title.unwrap_or_else(|| "未命名发现".into()),
                    severity: severity.unwrap_or_else(|| "info".into()),
                    status: status.unwrap_or_else(|| "open".into()),
                    target: target.unwrap_or_default(),
                    description,
                    confidence,
                    false_positive_status: fp,
                    evidence_ids: evidence_ids
                        .as_array()
                        .map(|ids| {
                            ids.iter()
                                .filter_map(|id| id.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            },
        )
        .collect();
    findings.sort_by_key(|finding| severity_rank(&finding.severity));
    Ok(findings)
}

async fn load_evidence(pool: &PgPool, run_id: &str) -> Result<Vec<EvidenceRow>, String> {
    let rows: Vec<EvidenceRowTuple> = sqlx::query_as(
        "SELECT e.id, e.tool_call_id, tc.tool_name, e.source, e.content_hash,
                e.content_type, e.created_at
         FROM pent_evidence e
         LEFT JOIN pent_tool_calls tc ON tc.id=e.tool_call_id AND tc.run_id=e.run_id
         WHERE e.run_id=$1 ORDER BY e.created_at",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(id, tool_call_id, tool_name, source, content_hash, content_type, created_at)| {
                EvidenceRow {
                    id,
                    tool_call_id,
                    tool_name,
                    source,
                    content_hash,
                    content_type: content_type.unwrap_or_else(|| "text/plain".into()),
                    created_at,
                }
            },
        )
        .collect())
}

async fn load_http_traffic(pool: &PgPool, run_id: &str) -> Result<Vec<HttpTrafficRow>, String> {
    let rows: Vec<(
        Option<String>,
        String,
        String,
        Value,
        Option<String>,
        Option<i32>,
        Option<String>,
        Option<i64>,
    )> = sqlx::query_as(
        "SELECT tool_call_id, method, url, request_headers, request_body,
                response_status, response_body, duration_ms
         FROM pent_http_traffic WHERE run_id=$1 ORDER BY created_at",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(
                tool_call_id,
                method,
                url,
                request_headers,
                request_body,
                response_status,
                response_body,
                duration_ms,
            )| HttpTrafficRow {
                tool_call_id,
                method,
                url,
                request_headers,
                request_body,
                response_status,
                response_body,
                duration_ms,
            },
        )
        .collect())
}

/// Render PoC (Proof of Concept) replication instructions for a finding.
/// Generates curl commands from the HTTP traffic records associated with
/// the finding's evidence. The output is intentionally text-only — the
/// user must manually review and execute any command.
fn render_poc_section(out: &mut String, traffic: &[&HttpTrafficRow]) {
    out.push_str("\n#### 复现指引（PoC）\n\n");
    out.push_str("> 以下 curl 命令由平台从已存储的 HTTP 流量记录中确定性生成，");
    out.push_str("未经模型加工。请在实际执行前确认目标、参数和授权范围。\n\n");

    for (i, row) in traffic.iter().enumerate() {
        // Build curl command
        let mut curl = format!("curl -X {} '{}'", row.method, escape_shell(&row.url));
        // Add headers
        if let Some(headers) = row.request_headers.as_object() {
            for (key, value) in headers {
                let header_value = match value {
                    Value::String(s) => s.as_str(),
                    other => &other.to_string(),
                };
                // Skip host and content-length (curl adds them automatically)
                if key.to_lowercase() == "host" || key.to_lowercase() == "content-length" {
                    continue;
                }
                curl.push_str(&format!(
                    " \\\n  -H '{}: {}'",
                    key,
                    escape_shell(header_value)
                ));
            }
        }
        // Add body
        if let Some(body) = &row.request_body {
            if !body.is_empty() && body != "null" {
                curl.push_str(&format!(" \\\n  -d '{}'", escape_shell(body)));
            }
        }
        // Add metadata comment
        out.push_str(&format!(
            "**请求 {}** (`{} {}`){}\n\n",
            i + 1,
            row.method,
            row.url,
            row.response_status
                .map(|s| format!(" → HTTP {s}"))
                .unwrap_or_default()
        ));
        out.push_str("```bash\n");
        out.push_str(&curl);
        out.push_str("\n```\n\n");
        // Response summary
        if let Some(status) = row.response_status {
            out.push_str(&format!("- 响应状态：{status}\n"));
        }
        if let Some(duration) = row.duration_ms {
            out.push_str(&format!("- 响应耗时：{duration}ms\n"));
        }
        if let Some(body) = &row.response_body {
            let preview = truncate(
                &crate::sanitize_sensitive_data(body),
                EVIDENCE_PREVIEW_MAX_CHARS / 2,
            );
            if !preview.is_empty() {
                out.push_str("\n<details>\n<summary>响应正文预览</summary>\n\n```\n");
                out.push_str(&preview);
                out.push_str("\n```\n</details>\n\n");
            }
        }
        out.push('\n');
    }
}

/// Minimal shell escaping for single-quoted strings in curl commands.
fn escape_shell(value: &str) -> String {
    value.replace('\'', "'\\''")
}

async fn load_tool_calls(pool: &PgPool, run_id: &str) -> Result<Vec<ToolCallRow>, String> {
    let rows: Vec<ToolCallRowTuple> = sqlx::query_as(
        "SELECT tool_name, executor_backend, started_at, duration_ms, exit_code,
                permission_decision
         FROM pent_tool_calls WHERE run_id=$1 ORDER BY started_at NULLS LAST, created_at",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(tool_name, backend, started_at, duration_ms, exit_code, decision)| ToolCallRow {
                tool_name,
                executor_backend: backend.unwrap_or_else(|| "unknown".into()),
                started_at,
                duration_ms,
                exit_code,
                permission_decision: decision.unwrap_or_else(|| "-".into()),
            },
        )
        .collect())
}

fn render_markdown(
    run_id: &str,
    run: &RunSnapshot,
    findings: &[FindingRow],
    evidence: &[EvidenceRow],
    tool_calls: &[ToolCallRow],
    http_traffic: &[HttpTrafficRow],
    narrative: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("# 渗透测试运行报告\n\n");
    out.push_str("> 本报告由平台基于数据库中的证据与漏洞记录确定性生成，未经过模型改写；");
    out.push_str("所有结论均可通过证据索引回溯至原始工具输出。\n\n");

    out.push_str("## 一、运行概要\n\n");
    out.push_str(&format!("- 运行 ID：`{run_id}`\n"));
    out.push_str(&format!("- 测试目标：{}\n", or_dash(&run.objective)));
    out.push_str(&format!("- 运行状态：{}\n", run.status));
    out.push_str(&format!("- 创建时间：{}\n", fmt_time(run.created_at)));
    out.push_str(&format!("- 开始时间：{}\n", fmt_time(run.started_at)));
    out.push_str(&format!("- 完成时间：{}\n", fmt_time(run.completed_at)));
    out.push_str(&format!("- 报告生成时间：{}\n", fmt_time(Some(Utc::now()))));
    let targets = run
        .scope
        .get("targets")
        .and_then(Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter_map(|target| target.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !targets.is_empty() {
        out.push_str(&format!("- 授权范围：{}\n", targets.join("、")));
    }

    out.push_str("\n## 二、结论概览\n\n");
    let confirmed = findings
        .iter()
        .filter(|finding| finding.false_positive_status == "confirmed")
        .count();
    out.push_str(&format!(
        "本次运行共记录 **{} 条漏洞发现**（人工确认 {} 条）、**{} 条证据**、**{} 次工具调用**。\n\n",
        findings.len(),
        confirmed,
        evidence.len(),
        tool_calls.len()
    ));
    if !findings.is_empty() {
        out.push_str("| 严重级别 | 数量 |\n|---|---|\n");
        for severity in ["critical", "high", "medium", "low", "info"] {
            let count = findings
                .iter()
                .filter(|finding| finding.severity == severity)
                .count();
            if count > 0 {
                out.push_str(&format!("| {} | {} |\n", severity_label(severity), count));
            }
        }
        out.push('\n');
        out.push_str("| 证据成熟度 | 数量 |\n|---|---|\n");
        for level in ["E4", "E3", "E2", "E1", "E0"] {
            let count = findings
                .iter()
                .filter(|finding| maturity_level(finding, evidence).0 == level)
                .count();
            if count > 0 {
                out.push_str(&format!("| {level} | {count} |\n"));
            }
        }
        out.push('\n');
    }

    out.push_str("## 三、漏洞清单\n\n");
    if findings.is_empty() {
        out.push_str("本次运行未产生漏洞发现。\n\n");
    }
    for finding in findings {
        let (maturity, maturity_label) = maturity_level(finding, evidence);
        out.push_str(&format!(
            "### [{}] {}\n\n",
            severity_label(&finding.severity),
            finding.title
        ));
        out.push_str(&format!("- 发现 ID：`{}`\n", finding.id));
        out.push_str(&format!("- 状态：{}\n", finding.status));
        out.push_str(&format!(
            "- 评审：{}\n",
            review_label(&finding.false_positive_status)
        ));
        out.push_str(&format!("- 证据成熟度：{maturity}（{maturity_label}）\n"));
        if !finding.target.is_empty() {
            out.push_str(&format!("- 目标：`{}`\n", finding.target));
        }
        out.push_str(&format!("- 置信度：{:.2}\n", finding.confidence));
        if !finding.evidence_ids.is_empty() {
            let refs = finding
                .evidence_ids
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join("、");
            out.push_str(&format!("- 证据引用：{refs}\n"));
        }
        if !finding.description.is_empty() {
            out.push_str(&format!("\n{}\n", finding.description));
        }
        let related_traffic: Vec<&HttpTrafficRow> = http_traffic
            .iter()
            .filter(|row| {
                finding.evidence_ids.iter().any(|ev_id| {
                    row.tool_call_id.as_deref() == Some(ev_id.as_str())
                        || evidence.iter().any(|ev| {
                            ev.id == *ev_id
                                && ev.tool_call_id.as_deref() == row.tool_call_id.as_deref()
                        })
                })
            })
            .collect();
        if !related_traffic.is_empty() {
            render_poc_section(&mut out, &related_traffic);
        }
        out.push('\n');
    }

    out.push_str("## 四、证据索引\n\n");
    if evidence.is_empty() {
        out.push_str("本次运行未留存证据。\n\n");
    }
    for item in evidence {
        let preview = truncate(
            &crate::sanitize_sensitive_data(&item.source),
            EVIDENCE_PREVIEW_MAX_CHARS,
        );
        out.push_str(&format!("### 证据 `{}`\n\n", item.id));
        out.push_str(&format!("- SHA-256：`{}`\n", item.content_hash));
        out.push_str(&format!("- 内容类型：{}\n", item.content_type));
        if let Some(tool_name) = &item.tool_name {
            out.push_str(&format!("- 来源工具：`{tool_name}`\n"));
        }
        if let Some(tool_call_id) = &item.tool_call_id {
            out.push_str(&format!("- 来源工具调用：`{tool_call_id}`\n"));
        }
        out.push_str(&format!("- 采集时间：{}\n", fmt_time(item.created_at)));
        if !preview.is_empty() {
            out.push_str("\n```text\n");
            out.push_str(&preview);
            if !preview.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n");
        }
        out.push('\n');
    }

    out.push_str("## 五、工具执行时间线\n\n");
    if tool_calls.is_empty() {
        out.push_str("无工具调用记录。\n\n");
    } else {
        out.push_str("| 工具 | 执行后端 | 开始时间 | 耗时(ms) | 退出码 | 审批 |\n|---|---|---|---|---|---|\n");
        for call in tool_calls {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                call.tool_name,
                call.executor_backend,
                fmt_time(call.started_at),
                call.duration_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".into()),
                call.exit_code
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".into()),
                call.permission_decision,
            ));
        }
        out.push('\n');
    }

    out.push_str("## 六、范围与策略快照\n\n");
    out.push_str("```json\n");
    out.push_str(&truncate(
        &serde_json::to_string_pretty(&serde_json::json!({
            "scope": run.scope,
            "policy": run.policy,
            "budget": run.budget,
        }))
        .unwrap_or_default(),
        SNAPSHOT_MAX_CHARS,
    ));
    out.push_str("\n```\n");

    if let Some(narrative) = narrative.filter(|value| !value.trim().is_empty()) {
        out.push_str("\n## 七、分析师总结（LLM 生成）\n\n");
        out.push_str("> 以下内容由模型生成，仅供阅读参考；事实结论以第三、四节为准。\n\n");
        out.push_str(narrative.trim());
        out.push('\n');
    }
    out
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        "info" => 4,
        _ => 5,
    }
}

fn severity_label(severity: &str) -> &'static str {
    match severity {
        "critical" => "严重",
        "high" => "高危",
        "medium" => "中危",
        "low" => "低危",
        "info" => "信息",
        _ => "未知",
    }
}

fn review_label(status: &str) -> &'static str {
    match status {
        "confirmed" => "已确认",
        "false_positive" => "误报",
        _ => "未评审",
    }
}

/// Evidence maturity E0–E4: E0 self-reported, E1 cited but not projectable,
/// E2 verbatim-projectable, E3 cross-tool corroborated, E4 human-confirmed.
fn maturity_level(finding: &FindingRow, evidence: &[EvidenceRow]) -> (&'static str, &'static str) {
    if finding.false_positive_status == "confirmed" {
        return ("E4", "人工评审确认");
    }
    let related: Vec<&EvidenceRow> = finding
        .evidence_ids
        .iter()
        .filter_map(|id| evidence.iter().find(|item| &item.id == id))
        .collect();
    if related.is_empty() {
        return ("E0", "无关联证据");
    }
    let mut categories = std::collections::HashSet::new();
    for item in &related {
        if let Some(category) = item.tool_name.as_deref().and_then(corroboration_category) {
            categories.insert(category);
        }
    }
    if categories.len() >= 2 {
        return ("E3", "多工具类别交叉印证");
    }
    let claims = format!("{} {}", finding.title, finding.description);
    let assertions = crate::conversation::claim_gate::extract_assertions(&claims);
    if !assertions.is_empty()
        && assertions.iter().all(|token| {
            related
                .iter()
                .any(|item| item.source.contains(token.as_str()))
        })
    {
        return ("E2", "关键断言可在证据中逐字投影");
    }
    ("E1", "已引用证据，关键断言未逐字投影")
}

/// Tool categories that count toward cross-tool corroboration (mirrors the
/// runtime's completion-gate classification; reference tools don't count).
fn corroboration_category(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "http_request" | "http_probe" | "dns_lookup" | "web_crawl" | "web_fingerprint"
        | "web_url_collect" | "js_collect" => Some("recon"),
        "dir_fuzz" | "js_analyze" | "file_upload_validate" => Some("validation"),
        _ => None,
    }
}

fn fmt_time(value: Option<DateTime<Utc>>) -> String {
    value
        .map(|time| time.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "-".into())
}

fn or_dash(value: &str) -> String {
    if value.trim().is_empty() {
        "-".into()
    } else {
        value.to_string()
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}\n…（已截断，完整内容见证据详情）")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_run() -> RunSnapshot {
        RunSnapshot {
            objective: "对 192.0.2.10 进行授权渗透测试".into(),
            scope: json!({"targets": ["192.0.2.10"], "confirmed": true}),
            policy: json!({"max_risk": "medium"}),
            budget: json!({"max_turns": 50}),
            status: "completed".into(),
            created_at: None,
            started_at: None,
            completed_at: None,
        }
    }

    #[test]
    fn severity_rank_orders_critical_first() {
        assert!(severity_rank("critical") < severity_rank("high"));
        assert!(severity_rank("high") < severity_rank("medium"));
        assert!(severity_rank("medium") < severity_rank("low"));
        assert!(severity_rank("low") < severity_rank("info"));
        assert!(severity_rank("info") < severity_rank("unknown"));
    }

    #[test]
    fn renders_findings_sorted_by_severity_with_evidence_refs() {
        let mut findings = vec![
            FindingRow {
                id: "f-low".into(),
                title: "低危问题".into(),
                severity: "low".into(),
                status: "open".into(),
                target: String::new(),
                description: String::new(),
                confidence: 0.5,
                false_positive_status: "unreviewed".into(),
                evidence_ids: vec!["ev-2".into()],
            },
            FindingRow {
                id: "f-high".into(),
                title: "高危 SQL 注入".into(),
                severity: "high".into(),
                status: "open".into(),
                target: "http://192.0.2.10/login".into(),
                description: "登录接口存在注入".into(),
                confidence: 0.9,
                false_positive_status: "confirmed".into(),
                evidence_ids: vec!["ev-1".into()],
            },
        ];
        // Production ordering happens in `load_findings` after the DB fetch.
        findings.sort_by_key(|finding| severity_rank(&finding.severity));
        let evidence = vec![EvidenceRow {
            id: "ev-1".into(),
            tool_call_id: Some("tc-1".into()),
            tool_name: Some("http_request".into()),
            source: "HTTP 200 ... sql syntax".into(),
            content_hash: "abc123".into(),
            content_type: "text/plain".into(),
            created_at: None,
        }];
        let report = render_markdown("run-1", &sample_run(), &findings, &evidence, &[], &[], None);
        let high_pos = report.find("高危 SQL 注入").expect("high finding");
        let low_pos = report.find("低危问题").expect("low finding");
        assert!(high_pos < low_pos, "high severity must come first");
        assert!(report.contains("证据引用：`ev-1`"));
        assert!(report.contains("证据 `ev-1`"));
        assert!(report.contains("人工确认 1 条"));
    }

    #[test]
    fn renders_empty_run_honestly() {
        let report = render_markdown("run-2", &sample_run(), &[], &[], &[], &[], None);
        assert!(report.contains("未产生漏洞发现"));
        assert!(report.contains("未留存证据"));
        assert!(report.contains("无工具调用记录"));
    }

    #[test]
    fn maturity_follows_e0_to_e4_rules() {
        let finding = |fp: &str, evidence_ids: Vec<String>| FindingRow {
            id: "f".into(),
            title: "注入特征 `sql syntax error`".into(),
            severity: "high".into(),
            status: "open".into(),
            target: String::new(),
            description: String::new(),
            confidence: 0.9,
            false_positive_status: fp.into(),
            evidence_ids,
        };
        let evidence = vec![
            EvidenceRow {
                id: "ev-1".into(),
                tool_call_id: None,
                tool_name: Some("http_request".into()),
                source: "response contains sql syntax error here".into(),
                content_hash: "h1".into(),
                content_type: "text/plain".into(),
                created_at: None,
            },
            EvidenceRow {
                id: "ev-2".into(),
                tool_call_id: None,
                tool_name: Some("dir_fuzz".into()),
                source: "other output".into(),
                content_hash: "h2".into(),
                content_type: "text/plain".into(),
                created_at: None,
            },
        ];
        // E4: human confirmed wins over everything
        assert_eq!(
            maturity_level(&finding("confirmed", vec!["ev-1".into()]), &evidence).0,
            "E4"
        );
        // E0: referenced evidence no longer present
        assert_eq!(
            maturity_level(&finding("unreviewed", vec!["ev-x".into()]), &evidence).0,
            "E0"
        );
        // E3: two corroborating tool categories
        assert_eq!(
            maturity_level(
                &finding("unreviewed", vec!["ev-1".into(), "ev-2".into()]),
                &evidence
            )
            .0,
            "E3"
        );
        // E2: single category but the quoted assertion projects verbatim
        assert_eq!(
            maturity_level(&finding("unreviewed", vec!["ev-1".into()]), &evidence).0,
            "E2"
        );
        // E1: evidence cited but assertion absent
        assert_eq!(
            maturity_level(&finding("unreviewed", vec!["ev-2".into()]), &evidence).0,
            "E1"
        );
    }

    #[test]
    fn narrative_is_labelled_as_llm_generated() {
        let report = render_markdown(
            "run-3",
            &sample_run(),
            &[],
            &[],
            &[],
            &[],
            Some("模型总结内容"),
        );
        assert!(report.contains("分析师总结（LLM 生成）"));
        assert!(report.contains("模型总结内容"));
    }

    #[test]
    fn truncate_marks_overflow() {
        let long = "x".repeat(EVIDENCE_PREVIEW_MAX_CHARS + 10);
        let truncated = truncate(&long, EVIDENCE_PREVIEW_MAX_CHARS);
        assert!(truncated.contains("已截断"));
        assert!(truncated.chars().count() < long.chars().count() + 30);
    }

    #[tokio::test]
    async fn generates_and_stores_report_end_to_end() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
        if std::env::var("MINIO_ENDPOINT").is_err() {
            return;
        }
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        let pool = kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database");
        let run_id = "report-it-run";

        for table in [
            "pent_artifacts",
            "pent_findings",
            "pent_evidence",
            "pent_tool_calls",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE run_id=$1"))
                .bind(run_id)
                .execute(&pool)
                .await
                .expect("cleanup seed rows");
        }
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup seed run");
        sqlx::query(
            "INSERT INTO pent_runs(id, task_id, tenant_id, user_id, status,
                                   objective_snapshot, scope_snapshot, policy_snapshot, budget_snapshot)
             VALUES ($1,'report-it-task',0,'it-tester','completed',
                     '对 192.0.2.10 进行授权渗透测试',
                     '{\"targets\":[\"192.0.2.10\"],\"confirmed\":true}'::jsonb,'{}','{}')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed run");
        sqlx::query(
            "INSERT INTO pent_tool_calls(id, run_id, tool_name, executor_backend,
                                         started_at, duration_ms, exit_code, permission_decision)
             VALUES ('report-it-tc1',$1,'http_request','container',now(),120,0,'auto')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed tool call");
        sqlx::query(
            "INSERT INTO pent_evidence(id, run_id, tool_call_id, source, content_hash, content_type)
             VALUES ('report-it-ev1',$1,'report-it-tc1',
                     'HTTP/1.1 200 OK\nsql syntax error near login\nAuthorization: Bearer abc123',
                     'deadbeef','text/plain')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed evidence");
        sqlx::query(
            "INSERT INTO pent_findings(id, run_id, project_id, title, severity, status, target,
                                       evidence_ids, description, fingerprint, confidence,
                                       false_positive_status)
             VALUES ('report-it-f1',$1, (SELECT project_id FROM pent_runs WHERE id=$1), 'SQL 注入','high','open','http://192.0.2.10/login',
                     '[\"report-it-ev1\"]'::jsonb,'登录接口存在注入','fp-it-1',0.9,'confirmed')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed finding");

        let artifact = generate_run_report(&pool, run_id, Some("模型总结内容"))
            .await
            .expect("generate report");
        assert_eq!(artifact.sha256.len(), 64);
        assert!(artifact.size_bytes > 0);

        let stored: Option<(String, i64)> =
            sqlx::query_as("SELECT object_key, size_bytes FROM pent_artifacts WHERE id=$1")
                .bind(&artifact.artifact_id)
                .fetch_optional(&pool)
                .await
                .expect("query artifact row");
        let (object_key, size) = stored.expect("artifact row persisted");
        assert_eq!(object_key, artifact.object_key);
        assert_eq!(size, artifact.size_bytes);

        let (content_type, bytes) = crate::infrastructure::object_store::ObjectStore::from_env()
            .get_object(&artifact.object_key)
            .await
            .expect("download from MinIO");
        assert!(content_type.contains("text/markdown"));
        let markdown = String::from_utf8(bytes).expect("utf8 report");
        assert!(markdown.contains("SQL 注入"));
        assert!(markdown.contains("report-it-ev1"));
        assert!(markdown.contains("模型总结内容"));
        assert!(markdown.contains("***REDACTED***"));
        assert!(!markdown.contains("abc123"), "敏感信息必须脱敏");

        for table in [
            "pent_artifacts",
            "pent_findings",
            "pent_evidence",
            "pent_tool_calls",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE run_id=$1"))
                .bind(run_id)
                .execute(&pool)
                .await
                .expect("cleanup after test");
        }
        sqlx::query("DELETE FROM pent_runs WHERE id=$1")
            .bind(run_id)
            .execute(&pool)
            .await
            .expect("cleanup run after test");
    }
}
