//! LLM-04: Structured output validator — JSON schema enforcement.
use kairos_platform_pent_api::{ConversationIntent, ConversationPlanUpdate};

/// Validate model output against schema. Returns MODEL_OUTPUT_INVALID on failure.
pub fn validate_plan_update(raw: &str) -> Result<ConversationPlanUpdate, String> {
    match serde_json::from_str::<ConversationPlanUpdate>(raw) {
        Ok(p) => Ok(p),
        Err(e) => {
            // One retry with fixed JSON
            let trimmed = raw.trim();
            if !trimmed.starts_with('{') {
                return Err(format!("MODEL_OUTPUT_INVALID: not JSON — {}", e));
            }
            serde_json::from_str(trimmed)
                .map_err(|e2| format!("MODEL_OUTPUT_INVALID: retry failed — {}", e2))
        }
    }
}

/// Validate intent output from model.
pub fn validate_intent(raw: &str) -> Result<ConversationIntent, String> {
    serde_json::from_str::<ConversationIntent>(raw.trim())
        .map_err(|e| format!("MODEL_OUTPUT_INVALID: intent parse — {}", e))
}
