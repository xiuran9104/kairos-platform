use kairos_platform_pent_api::cognition::ActionReflection;
pub fn evaluate_action(
    action_id: &str,
    useful: bool,
    info_gain: f32,
    tool_quality: f32,
) -> ActionReflection {
    ActionReflection {
        reflection_id: uuid::Uuid::new_v4().to_string(),
        action_id: action_id.into(),
        useful,
        information_gain: info_gain,
        tool_selection_quality: tool_quality,
        result_quality: if useful { 0.8 } else { 0.3 },
        retry: !useful,
        reason_codes: vec![],
        lessons: vec![],
    }
}
