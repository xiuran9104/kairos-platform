//! Context compaction policy and immutable user-intent ledger for agent runtimes.

use serde_json::{Value, json};

pub struct CompactionInput {
    pub older: Vec<Value>,
    pub retained: Vec<Value>,
    pub user_ledger: String,
}

pub fn prepare(messages: &[Value], budget: u64) -> Option<CompactionInput> {
    let estimated = messages.iter().map(estimate).sum::<u64>();
    if estimated < budget.saturating_mul(8) / 10 || messages.len() <= 14 {
        return None;
    }
    let split = messages.len().saturating_sub(12).max(1);
    let older = messages[..split].to_vec();
    let retained = messages[split..].to_vec();
    let ledger = messages
        .iter()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str())
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.chars().take(16_000).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n---\n");
    Some(CompactionInput {
        older,
        retained,
        user_ledger: ledger.chars().take(96_000).collect(),
    })
}

pub fn merge(summary: String, input: CompactionInput) -> Vec<Value> {
    let mut output = vec![
        json!({"role":"system","content":format!("## 历史上下文压缩摘要\n{summary}\n\n## 用户原始意图账本（不可被摘要覆盖）\n{}", input.user_ledger)}),
    ];
    output.extend(input.retained);
    output
}

pub fn fallback_summary(messages: &[Value]) -> String {
    messages
        .iter()
        .filter_map(|m| {
            let role = m["role"].as_str()?;
            let content = m["content"].as_str()?;
            Some(format!(
                "[{role}] {}",
                content.chars().take(600).collect::<String>()
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn estimate(value: &Value) -> u64 {
    value
        .to_string()
        .chars()
        .map(|c| if c > '\u{2e80}' { 1_u64 } else { 1 })
        .sum::<u64>()
        / 3
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_recent_messages_and_user_ledger() {
        let messages = (0..30).map(|i| json!({"role":if i%2==0{"user"}else{"assistant"},"content":"长内容".repeat(200)})).collect::<Vec<_>>();
        let input = prepare(&messages, 1000).expect("should compact");
        assert_eq!(input.retained.len(), 12);
        assert!(input.user_ledger.contains("长内容"));
    }
}
