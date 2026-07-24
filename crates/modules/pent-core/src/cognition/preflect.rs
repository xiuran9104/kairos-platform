use kairos_platform_pent_api::{ActionProposal, PreFlectDirective, PreFlection};

pub fn deterministic_preflect(
    proposal: &ActionProposal,
    duplicate: bool,
    budget_available: bool,
    scope_valid: bool,
) -> PreFlection {
    let (directive, risks, summary) = if !scope_valid {
        (
            PreFlectDirective::Block,
            vec!["SCOPE_DENIED".into()],
            "目标不在授权范围".into(),
        )
    } else if !budget_available {
        (
            PreFlectDirective::Block,
            vec!["BUDGET_EXHAUSTED".into()],
            "执行预算不足".into(),
        )
    } else if duplicate {
        (
            PreFlectDirective::Replace,
            vec!["DUPLICATE_LOW_VALUE_ACTION".into()],
            "相同动作已经执行，建议替换能力".into(),
        )
    } else {
        (
            PreFlectDirective::Execute,
            vec![],
            "动作通过确定性事前检查".into(),
        )
    };
    PreFlection {
        reflection_id: uuid::Uuid::new_v4().to_string(),
        action_id: proposal.action_id.clone(),
        directive,
        expected_information_gain: if duplicate { 0.0 } else { 0.5 },
        confidence: 0.95,
        predicted_outcomes: proposal.expected_outputs.clone(),
        risk_codes: risks,
        alternatives: vec![],
        requires_approval: matches!(proposal.risk_level.as_str(), "high" | "critical"),
        reasoning_summary: summary,
    }
}
