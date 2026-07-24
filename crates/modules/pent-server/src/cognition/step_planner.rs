use kairos_platform_pent_api::cognition::*;
pub struct StepPlanner;
impl StepPlanner {
    pub fn propose(node: &PlanNode) -> ActionProposal {
        ActionProposal {
            action_id: uuid::Uuid::new_v4().to_string(),
            plan_id: String::new(),
            plan_version: 1,
            node_id: node.node_id.clone(),
            capability: node.capability.clone().unwrap_or_default(),
            target: node.target.clone().unwrap_or_default(),
            argument_template: node.success_criteria.clone(),
            expected_outputs: vec![],
            risk_level: "low".into(),
        }
    }
}
