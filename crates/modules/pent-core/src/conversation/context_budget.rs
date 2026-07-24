use kairos_platform_pent_api::{AgentMessage, ConversationContext};

pub const DEFAULT_CONTEXT_TOKEN_BUDGET: u64 = 12_000;

pub fn estimate_tokens(text: &str) -> u64 {
    text.chars().count().div_ceil(3) as u64
}

pub fn trim_recent_messages(
    mut context: ConversationContext,
    token_budget: u64,
) -> ConversationContext {
    let fixed_tokens = estimate_tokens(&context.system_instruction)
        + estimate_tokens(&context.objective_snapshot)
        + estimate_tokens(&context.run_summary)
        + estimate_tokens(&context.scope_snapshot.to_string())
        + estimate_tokens(&context.policy_snapshot.to_string());
    let available = token_budget.saturating_sub(fixed_tokens);
    let mut selected: Vec<AgentMessage> = Vec::new();
    let mut used = 0;
    for message in context.recent_messages.into_iter().rev() {
        let cost = estimate_tokens(&message.content);
        if used + cost > available {
            break;
        }
        used += cost;
        selected.push(message);
    }
    selected.reverse();
    context.recent_messages = selected;
    context.estimated_tokens = fixed_tokens + used;
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_newest_messages_within_budget() {
        let mut context = ConversationContext {
            context_version: 1,
            session_id: "s".into(),
            run_id: None,
            system_instruction: String::new(),
            objective_snapshot: String::new(),
            scope_snapshot: serde_json::json!({}),
            policy_snapshot: serde_json::json!({}),
            budget_snapshot: serde_json::json!({}),
            run_summary: String::new(),
            recent_messages: vec![message("old text"), message("new")],
            evidence_refs: vec![],
            memory_refs: vec![],
            tool_ids: vec![],
            estimated_tokens: 0,
        };
        context = trim_recent_messages(context, 4);
        assert_eq!(context.recent_messages.len(), 1);
        assert_eq!(context.recent_messages[0].content, "new");
    }

    fn message(content: &str) -> AgentMessage {
        AgentMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            content: content.into(),
            format: "text".into(),
        }
    }
}
