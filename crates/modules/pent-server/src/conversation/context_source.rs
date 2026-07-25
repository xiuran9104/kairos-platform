//! LLM-02: ContextSource — layered context assembly.
use crate::conversation::ports::ConversationContextSource;
use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::{AgentMessage, AgentTurnRequest, ConversationContext};

pub struct PgContextSource {
    pool: PgPool,
}
impl PgContextSource {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConversationContextSource for PgContextSource {
    async fn build(&self, req: &AgentTurnRequest) -> anyhow::Result<ConversationContext> {
        let session: (
            String,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            Option<String>,
            serde_json::Value,
        ) = sqlx::query_as(
            "SELECT objective, scope_proposal, COALESCE(confirmed_scope_snapshot, '{}'::jsonb),
                        constraints, active_run_id,
                        COALESCE(
                            (SELECT budget_snapshot FROM pent_runs WHERE id=active_run_id),
                            budget,
                            '{}'::jsonb
                        )
                 FROM pent_agent_sessions WHERE id = $1",
        )
        .bind(&req.session_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Pent 会话不存在"))?;
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, role, content, format
             FROM pent_conversation_messages
             WHERE session_id = $1
             ORDER BY created_at DESC, sequence DESC LIMIT 500",
        )
        .bind(&req.session_id)
        .fetch_all(&self.pool)
        .await?;
        let mut messages: Vec<AgentMessage> = rows
            .into_iter()
            .rev()
            .map(|(message_id, role, content, format)| AgentMessage {
                message_id,
                role,
                content,
                format,
            })
            .collect();
        messages.push(AgentMessage {
            message_id: format!("pending:{}", req.client_request_id),
            role: "user".into(),
            content: req.content.clone(),
            format: "text".into(),
        });

        let mut run_summary = sqlx::query_scalar::<_, String>(
            "SELECT run_summary FROM pent_context_snapshots
             WHERE session_id=$1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&req.session_id)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or_default();
        let mut memory_refs = Vec::new();
        if let Some(run_id) = session.4.as_deref() {
            let memories: Vec<(String, String)> = sqlx::query_as(
                "SELECT id::text,content FROM pent_knowledge_documents
                 WHERE source='conversation-memory' AND tags ? $1
                 ORDER BY created_at DESC LIMIT 12",
            )
            .bind(run_id)
            .fetch_all(&self.pool)
            .await?;
            if !memories.is_empty() {
                memory_refs = memories.iter().map(|(id, _)| id.clone()).collect();
                let recalled = memories
                    .iter()
                    .rev()
                    .map(|(_, content)| content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                run_summary.push_str("\n\n## 长期记忆召回\n");
                run_summary.push_str(&recalled);
            }
        }

        let estimated_tokens = messages
            .iter()
            .map(|m| m.content.len() as u64 / 4)
            .sum::<u64>()
            + 500;
        let scope_confirmed = session.2 != serde_json::json!({});
        let scope_snapshot = if scope_confirmed {
            session.2.clone()
        } else {
            session.1.clone()
        };
        let authorization_context =
            authorization_context_instruction(&scope_snapshot, scope_confirmed);

        Ok(ConversationContext {
            context_version: 1,
            session_id: req.session_id.clone(),
            run_id: session.4,
            system_instruction: format!(
                "{}\n\n{}",
                r#"你是 Pent AI 渗透测试智能体，能够使用工具在授权目标上执行安全测试。

## 核心原则
- **用工具执行，而不是描述如何执行**——用户需要结果，不是教程。
- LLM 驱动的攻击规划：先下载目标所有 JS 文件并分析 API 路由/鉴权/业务逻辑，再生成攻击计划。
- 先在授权范围内最大限度收集信息（JS 全量下载、端点提取、指纹识别），再执行漏洞验证。
- 所有发现记录到证据系统，所有工具输出作为证据关联。

## 可用工具
- http_request: HTTP 请求（GET/POST/PUT/DELETE），分析响应内容
- http_probe: 探测 HTTP/HTTPS 服务存活状态和指纹
- dns_lookup: DNS 查询（A/AAAA/CNAME/MX/NS/TXT）
- web_crawl / web_url_collect: 爬取页面结构并收集 URL
- dir_fuzz: 在已确认授权范围内执行受控目录和文件发现
- js_collect: 收集页面引用的 JavaScript 文件
- js_analyze: 从 JavaScript 中提取 API、上传路径、鉴权和技术线索
- file_upload_validate: 明确授权后用无害标记文件验证上传和存储行为
- knowledge_search: 检索平台 Wiki 验证方法；未验证线索、工具失败或缺少验证路径时必须调用

## 行为准则
1. 用户给出目标后，**立即开始用工具执行信息收集**，不要反问"需要我执行吗"
2. 先下载目标的所有 JS 文件，提取 API 端点、鉴权逻辑、内部域名、密钥
3. 存活探测只是起点；继续执行页面爬取、受控目录发现和 JS 分析
4. 对发现的接口进行非破坏性验证；上传验证只能使用无害标记文件
5. 只有完成发现、分析、验证（或明确记录阻塞原因）后才能输出最终报告
6. 只用中文回复用户，用英文和工具交互
7. 如果目标明显超出授权范围，明确指出并停止对该目标的操作
8. 发现未验证线索时，先调用 knowledge_search 检索对应 Wiki，再根据方法使用可用工具持续验证
9. 每条未验证线索必须单独检索 Wiki，并至少执行两次非重复的实际工具验证；严重度只影响排序和评级，不能减少验证力度
10. 多次验证后仍无法确认时，只能标记为“可能存在”，并逐项写明已尝试方法、Evidence、无法确认原因和下一步复核思路
11. 最终报告区分已验证发现、未验证线索、安全配置问题和无安全影响的正常行为"#,
                authorization_context
            ),
            objective_snapshot: session.0,
            scope_snapshot,
            policy_snapshot: session.3,
            budget_snapshot: session.5,
            run_summary,
            recent_messages: messages,
            evidence_refs: vec![],
            memory_refs,
            tool_ids: vec![],
            estimated_tokens,
        })
    }
}

fn authorization_context_instruction(
    scope_snapshot: &serde_json::Value,
    confirmed: bool,
) -> String {
    if confirmed {
        format!(
            "## 当前授权状态（平台事实）\n\
             - 本会话授权范围已经由用户一次性明确确认：{}\n\
             - 不得再次询问“是否批准计划”或按扫描阶段重复索要授权；阶段不是审批边界。\n\
             - 已确认范围内的低风险和中风险信息收集、爬取、目录发现、JS 分析及非破坏性验证应连续执行。\n\
             - 当前对话工具统一使用本会话授权，不得为单个工具重复索要授权；只有准备访问新增目标时才重新确认范围。\n\
             - 不得声称会启动多个子代理；当前运行时使用工具调用循环，应准确描述实际执行方式。\n\
             - 计划只是内部执行进度，生成计划后立即调用工具，不要把计划作为等待用户回复的终点。",
            scope_snapshot
        )
    } else {
        format!(
            "## 当前授权状态（平台事实）\n\
             - 本会话尚未形成结构化授权快照，当前候选范围：{}\n\
             - 在授权确认前禁止调用网络工具。\n\
             - 用户可在一条消息中明确写出“已授权”并附目标 URL；确认一次后，不再按阶段重复确认。",
            scope_snapshot
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::authorization_context_instruction;

    #[test]
    fn confirmed_scope_forbids_repeated_stage_approval() {
        let instruction =
            authorization_context_instruction(&json!({"targets":["https://example.test"]}), true);
        assert!(instruction.contains("不得再次询问"));
        assert!(instruction.contains("阶段不是审批边界"));
        assert!(instruction.contains("https://example.test"));
    }
}
