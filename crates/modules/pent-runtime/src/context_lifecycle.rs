//! Shared context projection and checkpoint lifecycle for main and sub-agents.

use kairos_platform_pent_api::{AgentMessage, ConversationContext};

use crate::agent_loop::AgentLoopCheckpoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentContextRole {
    Main,
    SubAgent,
}

impl AgentContextRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::SubAgent => "sub",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContextProjectionPolicy {
    pub max_messages: usize,
    pub max_chars: usize,
    pub user_ledger_chars: usize,
}

impl ContextProjectionPolicy {
    pub const fn main() -> Self {
        Self {
            max_messages: 500,
            max_chars: 512_000,
            user_ledger_chars: 0,
        }
    }

    pub const fn sub_agent() -> Self {
        Self {
            max_messages: 80,
            max_chars: 128_000,
            user_ledger_chars: 24_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContextProjection {
    pub removed_messages: usize,
    pub estimated_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCheckpoint {
    pub loop_state: AgentLoopCheckpoint,
    pub orchestration: Option<String>,
    pub updated_at: Option<String>,
}

impl ContextCheckpoint {
    pub fn from_context(context: &ConversationContext) -> Self {
        Self::from_value(
            context
                .policy_snapshot
                .get("_checkpoint")
                .unwrap_or(&serde_json::Value::Null),
        )
    }

    pub fn from_value(value: &serde_json::Value) -> Self {
        let usize_value = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize
        };
        Self {
            loop_state: AgentLoopCheckpoint {
                next_round: usize_value("round"),
                request_count: usize_value("request_count"),
                token_count: value
                    .get("token_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                tool_call_count: usize_value("tool_call_count"),
                effective_observations: value
                    .get("effective_observations")
                    .or_else(|| value.get("successful_observations"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize,
                consecutive_failures: usize_value("consecutive_failures"),
            },
            orchestration: value
                .get("orchestration")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            updated_at: value
                .get("updated_at")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "round": self.loop_state.next_round,
            "request_count": self.loop_state.request_count,
            "token_count": self.loop_state.token_count,
            "tool_call_count": self.loop_state.tool_call_count,
            "effective_observations": self.loop_state.effective_observations,
            // Retain the legacy key while stored snapshots are still in use.
            "successful_observations": self.loop_state.effective_observations,
            "consecutive_failures": self.loop_state.consecutive_failures,
            "orchestration": self.orchestration,
            "updated_at": self.updated_at,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContextManager {
    role: AgentContextRole,
    policy: ContextProjectionPolicy,
}

impl ContextManager {
    pub const fn main() -> Self {
        Self {
            role: AgentContextRole::Main,
            policy: ContextProjectionPolicy::main(),
        }
    }

    pub const fn sub_agent() -> Self {
        Self {
            role: AgentContextRole::SubAgent,
            policy: ContextProjectionPolicy::sub_agent(),
        }
    }

    pub const fn sub_agent_with_user_ledger_chars(user_ledger_chars: usize) -> Self {
        let mut policy = ContextProjectionPolicy::sub_agent();
        policy.user_ledger_chars = user_ledger_chars;
        Self {
            role: AgentContextRole::SubAgent,
            policy,
        }
    }

    pub const fn with_policy(role: AgentContextRole, policy: ContextProjectionPolicy) -> Self {
        Self { role, policy }
    }

    /// Apply role projection once after an infrastructure source builds the
    /// context. Sub-agents receive a bounded immutable user-intent ledger.
    pub fn initialize(&self, context: &mut ConversationContext, ledger_message_id: Option<String>) {
        context.policy_snapshot["_agent_role"] = serde_json::json!(self.role.as_str());
        if self.policy.user_ledger_chars > 0
            && let Some(message_id) = ledger_message_id
            && !context
                .recent_messages
                .iter()
                .any(|message| message.message_id == message_id)
            && let Some(content) =
                user_intent_ledger(&context.recent_messages, self.policy.user_ledger_chars)
        {
            context.recent_messages.push(AgentMessage {
                message_id,
                role: "user".into(),
                content,
                format: "text".into(),
            });
        }
        self.project(context);
    }

    /// Bound the portable conversation context before every model call.
    /// Provider-specific tool-pair sanitation remains in the model adapter.
    pub fn project(&self, context: &mut ConversationContext) -> ContextProjection {
        let original_len = context.recent_messages.len();
        while context.recent_messages.len() > self.policy.max_messages
            || message_chars(&context.recent_messages) > self.policy.max_chars
        {
            let remove = oldest_group_len(&context.recent_messages);
            if remove == 0 || remove >= context.recent_messages.len() {
                break;
            }
            context.recent_messages.drain(..remove);
        }
        context.estimated_tokens = estimate_context_tokens(context);
        ContextProjection {
            removed_messages: original_len.saturating_sub(context.recent_messages.len()),
            estimated_tokens: context.estimated_tokens,
        }
    }

    pub fn restore_checkpoint(&self, context: &ConversationContext) -> ContextCheckpoint {
        ContextCheckpoint::from_context(context)
    }

    pub fn checkpoint(
        &self,
        context: &mut ConversationContext,
        loop_state: AgentLoopCheckpoint,
        run_summary: &str,
        orchestration: Option<String>,
        updated_at: Option<String>,
    ) -> ContextCheckpoint {
        context.run_summary = run_summary.to_string();
        let checkpoint = ContextCheckpoint {
            loop_state,
            orchestration,
            updated_at,
        };
        context.policy_snapshot["_checkpoint"] = checkpoint.to_value();
        self.project(context);
        checkpoint
    }
}

pub fn user_intent_ledger(messages: &[AgentMessage], max_chars: usize) -> Option<String> {
    if max_chars == 0 {
        return None;
    }
    let user = messages
        .iter()
        .filter(|message| message.role == "user")
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>();
    let combined = match user.as_slice() {
        [] => return None,
        [only] => (*only).to_string(),
        many => format!(
            "[最初目标]\n{}\n\n[当前指令]\n{}",
            many[0],
            many[many.len() - 1]
        ),
    };
    let clipped = combined.chars().take(max_chars).collect::<String>();
    Some(format!(
        "\n\n## 用户历史输入（原文，子代理必读）\n{clipped}"
    ))
}

fn oldest_group_len(messages: &[AgentMessage]) -> usize {
    if messages.is_empty() {
        return 0;
    }
    if messages[0].format != "tool_calls" {
        return 1;
    }
    1 + messages[1..]
        .iter()
        .take_while(|message| message.format == "tool_result")
        .count()
}

fn message_chars(messages: &[AgentMessage]) -> usize {
    messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum()
}

fn estimate_context_tokens(context: &ConversationContext) -> u64 {
    let chars = context.system_instruction.chars().count()
        + context.objective_snapshot.chars().count()
        + context.run_summary.chars().count()
        + message_chars(&context.recent_messages)
        + context.scope_snapshot.to_string().chars().count()
        + context.policy_snapshot.to_string().chars().count();
    u64::try_from(chars / 3 + 1).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(messages: Vec<AgentMessage>) -> ConversationContext {
        ConversationContext {
            context_version: 1,
            session_id: "session".into(),
            run_id: Some("run".into()),
            system_instruction: "system".into(),
            objective_snapshot: "objective".into(),
            scope_snapshot: serde_json::json!({}),
            policy_snapshot: serde_json::json!({}),
            budget_snapshot: serde_json::json!({}),
            run_summary: String::new(),
            recent_messages: messages,
            evidence_refs: Vec::new(),
            memory_refs: Vec::new(),
            tool_ids: Vec::new(),
            estimated_tokens: 0,
        }
    }

    fn message(id: &str, role: &str, format: &str, content: &str) -> AgentMessage {
        AgentMessage {
            message_id: id.into(),
            role: role.into(),
            content: content.into(),
            format: format.into(),
        }
    }

    #[test]
    fn projection_drops_complete_oldest_tool_group() {
        let mut context = context(vec![
            message("a", "assistant", "tool_calls", "calls"),
            message("r1", "tool", "tool_result", "one"),
            message("r2", "tool", "tool_result", "two"),
            message("latest", "user", "text", "keep"),
        ]);
        let manager = ContextManager::with_policy(
            AgentContextRole::Main,
            ContextProjectionPolicy {
                max_messages: 1,
                max_chars: 100,
                user_ledger_chars: 0,
            },
        );
        let projected = manager.project(&mut context);
        assert_eq!(projected.removed_messages, 3);
        assert_eq!(context.recent_messages[0].message_id, "latest");
    }

    #[test]
    fn sub_agent_initialization_adds_bounded_user_ledger_once() {
        let mut context = context(vec![
            message("first", "user", "text", "initial target"),
            message("latest", "user", "text", "current request"),
        ]);
        let manager = ContextManager::sub_agent();
        manager.initialize(&mut context, Some("ledger".into()));
        manager.initialize(&mut context, Some("ledger".into()));
        assert_eq!(context.policy_snapshot["_agent_role"], "sub");
        assert_eq!(
            context
                .recent_messages
                .iter()
                .filter(|message| message.message_id == "ledger")
                .count(),
            1
        );
    }

    #[test]
    fn checkpoint_round_trips_all_loop_counters() {
        let mut context = context(Vec::new());
        let manager = ContextManager::main();
        let state = AgentLoopCheckpoint {
            next_round: 4,
            request_count: 5,
            token_count: 6,
            tool_call_count: 7,
            effective_observations: 8,
            consecutive_failures: 2,
        };
        manager.checkpoint(
            &mut context,
            state,
            "summary",
            Some("direct".into()),
            Some("now".into()),
        );
        assert_eq!(manager.restore_checkpoint(&context).loop_state, state);
        assert_eq!(context.run_summary, "summary");
        assert!(context.estimated_tokens > 0);
    }
}
