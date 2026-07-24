use kairos_platform_pent_api::{ConversationIntent, ConversationMessageType};

/// Deterministic fallback used when the intent model is unavailable.
pub fn classify_intent(content: &str, run_active: bool) -> ConversationIntent {
    let normalized = content.to_lowercase();
    let kind = if contains_any(&normalized, &["停止", "终止", "stop", "cancel"]) {
        ConversationMessageType::Stop
    } else if contains_any(
        &normalized,
        &["总结", "进度", "到哪", "summary", "progress"],
    ) {
        ConversationMessageType::RequestSummary
    } else if contains_any(&normalized, &["复核", "复测", "review", "verify"]) {
        ConversationMessageType::RequestReview
    } else if run_active
        && contains_any(
            &normalized,
            &["优先", "重点", "改为", "调整", "steer", "focus"],
        )
    {
        ConversationMessageType::Steer
    } else {
        ConversationMessageType::Chat
    };
    ConversationIntent {
        requires_run_instruction: run_active && !matches!(kind, ConversationMessageType::Chat),
        requires_confirmation: matches!(kind, ConversationMessageType::Stop),
        confidence: if matches!(kind, ConversationMessageType::Chat) {
            0.6
        } else {
            0.9
        },
        kind,
    }
}

fn contains_any(value: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| value.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_running_focus_request_to_instruction() {
        let intent = classify_intent("优先检查登录接口", true);
        assert_eq!(intent.kind, ConversationMessageType::Steer);
        assert!(intent.requires_run_instruction);
    }
}
