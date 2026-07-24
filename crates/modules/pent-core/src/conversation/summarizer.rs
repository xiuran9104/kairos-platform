//! LLM-08: Conversation summarizer — long context compression.
use crate::knowledge::ConversationMemory;

/// Summarize old conversation entries to save context space.
pub fn summarize_memory(memory: &ConversationMemory, keep_recent: usize) -> String {
    if memory.entries.len() <= keep_recent {
        return String::new();
    }
    let old_count = memory.entries.len() - keep_recent;
    let old_messages: Vec<_> = memory.entries.iter().take(old_count).collect();
    let summary = format!(
        "前{}轮对话摘要: {}",
        old_count,
        old_messages
            .iter()
            .map(|m| format!(
                "[{}] {}",
                m.role,
                m.content.chars().take(80).collect::<String>()
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );
    summary
}

/// Quality metrics for conversation evaluation.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ConversationQuality {
    pub intent_accuracy: f64,
    pub structured_output_rate: f64,
    pub instruction_application_rate: f64,
    pub user_retry_rate: f64,
    pub first_token_latency_ms: u64,
    pub total_latency_ms: u64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
}
