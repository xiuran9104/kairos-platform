//! DesRed-compatible tool-level HITL / AI fence.

use std::{collections::HashSet, time::Duration};

use kairos_platform_ai_api::{ChatMessage, ChatRequest};
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitlMode {
    Approval,
    ReviewEdit,
}

impl HitlMode {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("review_edit") | Some("review-edit") => Self::ReviewEdit,
            _ => Self::Approval,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::ReviewEdit => "review_edit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitlReviewer {
    Human,
    AuditAgent,
}

impl HitlReviewer {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("audit_agent") | Some("audit-agent") => Self::AuditAgent,
            _ => Self::Human,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::AuditAgent => "audit_agent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HitlPolicy {
    pub enabled: bool,
    pub mode: HitlMode,
    pub reviewer: HitlReviewer,
    pub approval_ttl_seconds: i32,
    pub audit_model_id: Option<i64>,
    pub audit_system_prompt: String,
    pub audit_user_prompt: String,
    global_whitelist: HashSet<String>,
    session_whitelist: HashSet<String>,
}

impl HitlPolicy {
    pub fn new(
        enabled: bool,
        mode: HitlMode,
        reviewer: HitlReviewer,
        global_whitelist: impl IntoIterator<Item = String>,
        session_whitelist: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            enabled,
            mode,
            reviewer,
            approval_ttl_seconds: 1800,
            audit_model_id: None,
            audit_system_prompt: String::new(),
            audit_user_prompt: String::new(),
            global_whitelist: normalize(global_whitelist),
            session_whitelist: normalize(session_whitelist),
        }
    }

    pub fn requires_review(&self, tool_name: &str) -> bool {
        self.enabled && !self.is_whitelisted(tool_name)
    }

    pub fn is_whitelisted(&self, tool_name: &str) -> bool {
        let name = normalize_name(tool_name);
        self.global_whitelist.contains(&name) || self.session_whitelist.contains(&name)
    }
}

#[derive(Debug, Clone)]
pub enum HitlOutcome {
    Approved(String),
    Rejected(String),
}

#[derive(Clone)]
pub struct HitlGate {
    pool: PgPool,
}

impl HitlGate {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn review(
        &self,
        run_id: &str,
        session_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> anyhow::Result<HitlOutcome> {
        let policy = self.load_policy(run_id, session_id).await?;
        if !policy.requires_review(tool_name) {
            return Ok(HitlOutcome::Approved(arguments.to_string()));
        }
        let approval_id = format!("approval-{}", uuid::Uuid::new_v4());
        self.create_approval(
            &approval_id,
            run_id,
            tool_call_id,
            tool_name,
            arguments,
            &policy,
        )
        .await?;

        if policy.reviewer == HitlReviewer::AuditAgent {
            match self.audit(session_id, tool_name, arguments, &policy).await {
                Ok(decision) => {
                    self.store_audit_decision(&approval_id, &decision).await?;
                    return Ok(if decision.decision == "approve" {
                        HitlOutcome::Approved(
                            decision.arguments.unwrap_or_else(|| arguments.to_string()),
                        )
                    } else {
                        HitlOutcome::Rejected(
                            decision
                                .comment
                                .unwrap_or_else(|| "审计 Agent 已拒绝工具调用".into()),
                        )
                    });
                }
                Err(error) => {
                    tracing::warn!(approval_id, error = %error, "audit agent failed; falling back to human review");
                    sqlx::query("UPDATE pent_approvals SET reviewer='human', audit_response=$2, updated_at=now() WHERE id=$1")
                        .bind(&approval_id).bind(json!({"error": error.to_string(), "fallback": "human"}))
                        .execute(&self.pool).await?;
                }
            }
        }
        self.wait_for_human(&approval_id, policy.approval_ttl_seconds)
            .await
    }

    async fn load_policy(&self, run_id: &str, session_id: &str) -> anyhow::Result<HitlPolicy> {
        let row: (bool, String, String, Value, Option<i64>, String, String, i32, Value, Value) = sqlx::query_as(
            "SELECT COALESCE((r.policy_snapshot->'hitl'->>'enabled')::boolean, (s.constraints->'hitl'->>'enabled')::boolean, p.hitl_enabled, false),
                    COALESCE(r.policy_snapshot->'hitl'->>'mode', s.constraints->'hitl'->>'mode', p.hitl_mode, 'approval'),
                    COALESCE(r.policy_snapshot->'hitl'->>'reviewer', s.constraints->'hitl'->>'reviewer', p.hitl_reviewer, 'human'),
                    COALESCE(p.hitl_tool_whitelist, '[]'::jsonb), p.hitl_audit_model_id,
                    COALESCE(p.hitl_audit_system_prompt,''), COALESCE(p.hitl_audit_user_prompt,''),
                    COALESCE(p.approval_ttl_seconds,1800), COALESCE(s.constraints->'hitl'->'tool_whitelist','[]'::jsonb),
                    COALESCE(r.policy_snapshot->'hitl'->'tool_whitelist','[]'::jsonb)
             FROM pent_runs r JOIN pent_agent_sessions s ON s.id=$2
             LEFT JOIN LATERAL (SELECT * FROM pent_runtime_policies WHERE tenant_id IN (r.tenant_id,0) ORDER BY (tenant_id=r.tenant_id) DESC LIMIT 1) p ON true
             WHERE r.id=$1")
            .bind(run_id).bind(session_id).fetch_one(&self.pool).await?;
        let mut session = string_array(&row.8);
        session.extend(string_array(&row.9));
        let mut policy = HitlPolicy::new(
            row.0,
            HitlMode::parse(Some(&row.1)),
            HitlReviewer::parse(Some(&row.2)),
            string_array(&row.3),
            session,
        );
        policy.audit_model_id = row.4;
        policy.audit_system_prompt = row.5;
        policy.audit_user_prompt = row.6;
        policy.approval_ttl_seconds = row.7;
        Ok(policy)
    }

    async fn create_approval(
        &self,
        id: &str,
        run_id: &str,
        call_id: &str,
        name: &str,
        args: &str,
        policy: &HitlPolicy,
    ) -> anyhow::Result<()> {
        let parsed =
            serde_json::from_str::<Value>(args).unwrap_or_else(|_| Value::String(args.into()));
        sqlx::query("INSERT INTO pent_approvals(id,run_id,tool_call_id,tool_name,description,status,expires_at,hitl_mode,reviewer,original_arguments,effective_arguments,audit_request) VALUES($1,$2,$3,$4,$5,'pending',now()+make_interval(secs=>$6),$7,$8,$9,$9,$10)")
            .bind(id).bind(run_id).bind(call_id).bind(name).bind(format!("工具 {name} 等待审批"))
            .bind(policy.approval_ttl_seconds).bind(policy.mode.as_str()).bind(policy.reviewer.as_str())
            .bind(&parsed).bind(json!({"tool_name": name, "arguments": parsed, "mode": policy.mode.as_str()}))
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn audit(
        &self,
        session_id: &str,
        name: &str,
        args: &str,
        policy: &HitlPolicy,
    ) -> anyhow::Result<AuditDecision> {
        let model_id = match policy.audit_model_id {
            Some(id) => id,
            None => sqlx::query_scalar("SELECT COALESCE(NULLIF(s.constraints->>'model_id','')::bigint,(SELECT id FROM ai.model_configs WHERE type='chat' AND status=0 ORDER BY id LIMIT 1)) FROM pent_agent_sessions s WHERE s.id=$1")
                .bind(session_id).fetch_one(&self.pool).await?,
        };
        let system = if policy.audit_system_prompt.trim().is_empty() {
            DEFAULT_AUDIT_SYSTEM
        } else {
            &policy.audit_system_prompt
        };
        let user = if policy.audit_user_prompt.trim().is_empty() {
            format!(
                "工具名称：{name}\n参数：{args}\n模式：{}",
                policy.mode.as_str()
            )
        } else {
            policy
                .audit_user_prompt
                .replace("{{tool_name}}", name)
                .replace("{{arguments}}", args)
                .replace("{{mode}}", policy.mode.as_str())
        };
        let response = AiModelFactory::new(self.pool.clone())
            .chat(
                model_id,
                ChatRequest {
                    model: String::new(),
                    messages: vec![
                        ChatMessage {
                            role: "system".into(),
                            content: system.into(),
                        },
                        ChatMessage {
                            role: "user".into(),
                            content: user,
                        },
                    ],
                    temperature: Some(0.0),
                    max_tokens: Some(800),
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        parse_audit_decision(&response.content, policy.mode)
    }

    async fn store_audit_decision(&self, id: &str, decision: &AuditDecision) -> anyhow::Result<()> {
        let effective = decision
            .arguments
            .as_deref()
            .and_then(|v| serde_json::from_str::<Value>(v).ok());
        sqlx::query("UPDATE pent_approvals SET status='decided',decision=$2,decided_by='audit_agent',decision_comment=$3,effective_arguments=COALESCE($4,effective_arguments),audit_response=$5,resumed_at=now(),updated_at=now() WHERE id=$1")
            .bind(id).bind(&decision.decision).bind(&decision.comment).bind(effective)
            .bind(serde_json::to_value(decision)?).execute(&self.pool).await?;
        Ok(())
    }

    async fn wait_for_human(&self, id: &str, ttl: i32) -> anyhow::Result<HitlOutcome> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(ttl as u64);
        loop {
            let row: (String, Option<String>, Option<Value>, Option<String>) = sqlx::query_as("SELECT status,decision,effective_arguments,decision_comment FROM pent_approvals WHERE id=$1")
                .bind(id).fetch_one(&self.pool).await?;
            if row.0 == "decided" {
                return Ok(if row.1.as_deref() == Some("approve") {
                    HitlOutcome::Approved(
                        row.2.map(|v| v.to_string()).unwrap_or_else(|| "{}".into()),
                    )
                } else {
                    HitlOutcome::Rejected(row.3.unwrap_or_else(|| "人工审批已拒绝工具调用".into()))
                });
            }
            if row.0 == "expired" || tokio::time::Instant::now() >= deadline {
                sqlx::query("UPDATE pent_approvals SET status='expired',decision='reject',decision_comment='审批已超时',updated_at=now() WHERE id=$1 AND status='pending'").bind(id).execute(&self.pool).await?;
                return Ok(HitlOutcome::Rejected("审批已超时".into()));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditDecision {
    decision: String,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

const DEFAULT_AUDIT_SYSTEM: &str = "你是渗透测试工具调用的独立审计 Agent。检查目标范围、破坏性、参数和必要性。只输出 JSON：{\"decision\":\"approve|reject\",\"comment\":\"说明\",\"arguments\":\"可选的 JSON 参数字符串\"}。review_edit 模式可修订参数；approval 模式不得修订。";

fn parse_audit_decision(content: &str, mode: HitlMode) -> anyhow::Result<AuditDecision> {
    let trimmed = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let mut value: AuditDecision = serde_json::from_str(trimmed)?;
    value.decision = value.decision.trim().to_ascii_lowercase();
    if !matches!(value.decision.as_str(), "approve" | "reject") {
        anyhow::bail!("invalid audit decision")
    }
    if mode == HitlMode::Approval {
        value.arguments = None;
    }
    if let Some(args) = &value.arguments {
        serde_json::from_str::<Value>(args)
            .map_err(|_| anyhow::anyhow!("audit agent returned invalid edited arguments"))?;
    }
    Ok(value)
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn normalize(values: impl IntoIterator<Item = String>) -> HashSet<String> {
    values
        .into_iter()
        .map(|v| normalize_name(&v))
        .filter(|v| !v.is_empty())
        .collect()
}
fn normalize_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merges_whitelists_case_insensitively() {
        let policy = HitlPolicy::new(
            true,
            HitlMode::Approval,
            HitlReviewer::Human,
            ["read_file".into()],
            ["NMAP".into()],
        );
        assert!(!policy.requires_review("Read_File"));
        assert!(!policy.requires_review("nmap"));
        assert!(policy.requires_review("sqlmap"));
    }
    #[test]
    fn approval_mode_ignores_edited_arguments() {
        let result = parse_audit_decision(
            r#"{"decision":"approve","arguments":"{}"}"#,
            HitlMode::Approval,
        )
        .unwrap();
        assert!(result.arguments.is_none());
    }
}
