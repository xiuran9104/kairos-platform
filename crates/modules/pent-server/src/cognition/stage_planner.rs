//! HPM-03: Stage Planner — expands a TCG Stage into Task/Action subgraph.
use kairos_platform_pent_api::cognition::*;

pub struct StagePlanner;

impl StagePlanner {
    pub fn expand_stage(stage: &PlanNode, tool_schemas: &[String]) -> Vec<PlanNode> {
        let mut tasks = Vec::new();
        let stage_id = &stage.node_id;
        match stage.namespace.as_str() {
            "pentest" => match stage.objective.as_str() {
                o if o.contains("侦察") => {
                    tasks.push(Self::action(
                        format!("{}-t1", stage_id),
                        "tcp_scan",
                        stage.target.as_deref().unwrap_or(""),
                        1,
                    ));
                    tasks.push(Self::action(
                        format!("{}-t2", stage_id),
                        "http_probe",
                        stage.target.as_deref().unwrap_or(""),
                        2,
                    ));
                    tasks.push(Self::action(
                        format!("{}-t3", stage_id),
                        "dns_lookup",
                        stage.target.as_deref().unwrap_or(""),
                        3,
                    ));
                }
                o if o.contains("扫描") => {
                    tasks.push(Self::action(
                        format!("{}-t1", stage_id),
                        "scanner_nmap",
                        stage.target.as_deref().unwrap_or(""),
                        1,
                    ));
                    tasks.push(Self::action(
                        format!("{}-t2", stage_id),
                        "scanner_nuclei",
                        stage.target.as_deref().unwrap_or(""),
                        2,
                    ));
                }
                _ => {
                    tasks.push(Self::action(
                        format!("{}-t1", stage_id),
                        "http_request",
                        stage.target.as_deref().unwrap_or(""),
                        1,
                    ));
                }
            },
            _ => {
                let _ = tool_schemas;
                tasks.push(Self::action(
                    format!("{}-default", stage_id),
                    "builtin",
                    "",
                    1,
                ));
            }
        }
        tasks
    }

    fn action(id: String, capability: &str, target: &str, priority: i32) -> PlanNode {
        let parent = id.split('-').next().unwrap_or("stage").to_string();
        PlanNode {
            node_id: id,
            node_type: PlanNodeType::Action,
            parent_id: Some(parent),
            namespace: capability.into(),
            objective: format!("执行{}", capability),
            capability: Some(capability.into()),
            target: Some(target.into()),
            status: PlanNodeStatus::Pending,
            priority,
            success_criteria: serde_json::json!({"completed": true}),
            budget: PlanBudget {
                max_actions: 1,
                max_tool_calls: 3,
                max_duration_seconds: 300,
                max_tokens: 10000,
                max_cost: 5.0,
                max_concurrency: 1,
            },
            attempt: 0,
            max_attempts: 2,
            observation_refs: vec![],
            valid_until: None,
        }
    }
}
