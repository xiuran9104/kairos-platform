//! Context assembly for LLM calls. Ported from PentAGI.
use crate::knowledge::ConversationMemory;

#[derive(Debug, Clone)]
pub struct ContextBudget {
    pub max_tokens: usize,
    pub system_prompt_tokens: usize,
    pub reserved_output_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            system_prompt_tokens: 2000,
            reserved_output_tokens: 4000,
        }
    }
}

impl ContextBudget {
    pub fn available_for_history(&self) -> usize {
        self.max_tokens
            .saturating_sub(self.system_prompt_tokens + self.reserved_output_tokens)
    }
}

pub fn assemble_context(
    system_prompt: &str,
    memory: &ConversationMemory,
    budget: &ContextBudget,
    extra: Option<&str>,
) -> String {
    let mut ctx = format!("<system>\n{}\n</system>\n\n", system_prompt);
    if let Some(e) = extra {
        ctx.push_str(&format!("<context>\n{}\n</context>\n\n", e));
    }
    ctx.push_str(&memory.build_context());
    let available = budget.available_for_history();
    if ctx.len() > available * 4 {
        ctx = format!(
            "{}...\n[truncated]",
            &ctx[ctx.len().saturating_sub(available * 4)..]
        );
    }
    ctx
}

pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}
