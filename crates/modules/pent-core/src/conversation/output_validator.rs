use kairos_platform_pent_api::{AgentTurnResult, ConversationTurnStatus};

pub fn validate_turn_result(result: &AgentTurnResult) -> Result<(), Vec<&'static str>> {
    let mut errors = Vec::new();
    if result.turn_id.is_empty() {
        errors.push("turn_id_required");
    }
    if result.message.content.trim().is_empty()
        && result.status == ConversationTurnStatus::Completed
    {
        errors.push("completed_message_required");
    }
    if let Some(plan) = &result.plan_update {
        if !matches!(plan.directive.as_str(), "continue" | "replan" | "stop") {
            errors.push("invalid_plan_directive");
        }
    }
    if result.intent.requires_run_instruction && result.run_id.is_none() {
        errors.push("run_id_required_for_instruction");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
