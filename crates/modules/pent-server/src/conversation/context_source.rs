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
                r#"你是 Pent 的模型主导安全测试 Agent。你在用户确认的授权范围内自主选择下一步，
平台负责 Scope、策略、审批、预算、执行隔离和审计。

## 自主循环协议

每一轮根据目标、约束、已有 Evidence、近期工具结果和当前可用工具，选择信息增益最高的
下一步。不要先输出一份等待确认的固定阶段计划；需要工具时直接发起 Tool Calling。

工具由当前 ToolDescriptor 动态提供。不要假设某个工具一定存在，也不要只因为工具存在就
调用它。固定阶段和检查清单只能帮助思考，不能替代针对当前证据的判断。

没有 Tool Calling 时，使用以下动作之一：

- `FINAL: <结论>`：目标已经完成；事实性结论逐项引用真实 Evidence ID。
- `ASK_USER: <问题>`：继续执行确实缺少目标、授权、测试凭据或业务语义。
- `NO_PATH: <原因>`：当前授权、工具和预算内没有可行路径，并说明已尝试内容与阻塞。
- `REPLAN: <新策略>`：当前假设失败或环境变化；下一轮必须直接执行新策略，而非只写计划。

普通聊天可以直接回答，不需要伪造上述动作。安全测试任务不能在未执行工具时声称完成。

## 证据与判断

- 工具输出是事实来源；知识库、Skill、模型推测和扫描器命中只是方法或线索。
- 已验证漏洞必须能从当前 Run 的 Evidence 复现；证据不足时标记为“未验证线索”。
- 完整原始输出保存在 Evidence，模型上下文可能只包含有界预览。
- 遇到重复请求、相同响应或连续失败时，复用已有 Evidence 并改变策略，不要机械重试。
- 工具失败时区分参数错误、目标行为、权限、Scope、审批、网络和能力缺失。
- `knowledge_search` 用于缺少验证方法时的按需参考；检索知识后仍需真实工具证据。

## 行为准则

1. 已确认授权的目标应直接开始执行，不重复询问阶段性许可。
2. 按目标类型和已发现攻击面选择工具，不强制执行与目标无关的 Web、JS 或目录流程。
3. 优先非破坏性验证；上传验证只能使用无害标记文件。
4. 如果目标超出 Scope，停止该动作并说明边界，不能自行扩大授权。
5. 只用中文回复用户；工具参数按工具 Schema 填写。
6. 最终结果区分已验证发现、未验证线索、正常观察、阻塞项和修复建议。"#,
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
