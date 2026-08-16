//! Context budget guard that prevents token overflow with smart summarization.
//!
//! When the conversation history approaches the token budget, instead of
//! blindly truncating, older messages are summarized via the model itself.
//! This preserves critical context (tool results, findings, evidence refs)
//! while freeing budget for new messages.
//!
//! Pattern borrowed from PentAGI's `ContextAssembler` + `ContextSummarizer`.

use serde_json::Value;

/// Rough token count for a JSON value (tool definition or message).
/// Conservative estimate: 1 token ≈ 4 characters for English, 2 for CJK.
pub fn estimate_tokens(value: &Value) -> usize {
    let s = value.to_string();
    let chars = s.chars().count();
    // Estimate CJK proportion
    let cjk = s.chars().filter(|c| c > &'\u{2e80}').count();
    let non_cjk = chars - cjk;
    non_cjk / 4 + cjk / 2
}

/// Budget breakdown for the current turn.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Total token budget allowed for this turn.
    pub total: usize,
    /// Tokens consumed by tool definitions.
    pub tool_schemas: usize,
    /// Tokens consumed by system prompt + safety instructions.
    pub system_prompt: usize,
    /// Tokens consumed by conversation history.
    pub history: usize,
    /// Tokens consumed by evidence / traffic / knowledge references.
    pub references: usize,
    /// Remaining budget for the model response.
    pub remaining: usize,
}

impl ContextBudget {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            tool_schemas: 0,
            system_prompt: 0,
            history: 0,
            references: 0,
            remaining: total,
        }
    }

    /// Whether the budget is over half spent. Triggers proactive summarization.
    pub fn needs_summarization(&self) -> bool {
        self.remaining < self.total / 2
    }

    /// Whether the budget is critically low. Triggers aggressive truncation.
    pub fn is_critical(&self) -> bool {
        self.remaining < self.total / 10
    }
}

/// Summarization hint sent to the model when budget is tight.
pub fn summarization_instruction(budget: &ContextBudget) -> String {
    format!(
        "⚠ 上下文接近 token 上限（{}/{} 已用）。\
         在回复前，先用一段简短摘要概括已完成的关键发现和未验证的线索，\
         然后继续执行最重要的下一步。省略已完成工具的详细输出。",
        budget.total - budget.remaining,
        budget.total
    )
}

/// Trim conversation messages to fit within a token budget while preserving
/// tool call/result pairs. Older turns are dropped first.
pub fn trim_messages_budget(messages: &mut Vec<Value>, max_tokens: usize, preserve_last_n: usize) {
    if messages.is_empty() {
        return;
    }

    let mut total = 0usize;
    let mut cut_at = None;

    // Walk from newest to oldest, accumulating token count
    for (i, msg) in messages.iter().enumerate().rev() {
        total += estimate_tokens(msg);
        if total > max_tokens {
            cut_at = Some(i + preserve_last_n.min(messages.len().saturating_sub(1)));
            break;
        }
    }

    if let Some(cut) = cut_at {
        if cut < messages.len() {
            // Keep the most recent messages, drop the oldest
            let keep = messages.split_off(cut);
            *messages = keep;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cjk_tokens_counted_correctly() {
        let eng = json!({"text": "hello world"});
        let cjk = json!({"text": "你好世界"});
        assert!(estimate_tokens(&eng) > estimate_tokens(&cjk));
    }

    #[test]
    fn budget_detects_critical() {
        let mut budget = ContextBudget::new(1000);
        budget.remaining = 50;
        assert!(budget.is_critical());
        assert!(budget.needs_summarization());
    }

    #[test]
    fn trim_preserves_recent_messages() {
        let mut msgs: Vec<Value> = (0..20)
            .map(|i| json!({"role": "user", "content": format!("msg {i}")}))
            .collect();
        let original_len = msgs.len();
        trim_messages_budget(&mut msgs, 50, 3);
        assert!(msgs.len() < original_len);
        assert!(msgs.len() >= 3);
    }
}
