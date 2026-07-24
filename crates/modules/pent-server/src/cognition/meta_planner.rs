//! HPM-02: Meta-Planner — creates initial TaskCoordinationGraph.
use chrono::Utc;
use kairos_platform_pent_api::cognition::*;

pub struct MetaPlanner;

impl MetaPlanner {
    pub fn plan_pentest(run_id: &str, target: &str) -> TaskCoordinationGraph {
        let budget = PlanBudget {
            max_actions: 30,
            max_tool_calls: 50,
            max_duration_seconds: 3600,
            max_tokens: 200_000,
            max_cost: 100.0,
            max_concurrency: 4,
        };
        let nodes = vec![
            PlanNode {
                node_id: "stage-1".into(),
                node_type: PlanNodeType::Stage,
                parent_id: None,
                namespace: "pentest".into(),
                objective: "资产侦察".into(),
                capability: Some("network".into()),
                target: Some(target.into()),
                status: PlanNodeStatus::Pending,
                priority: 1,
                success_criteria: serde_json::json!({"ports_open":">0"}),
                budget: budget.clone(),
                attempt: 0,
                max_attempts: 3,
                observation_refs: vec![],
                valid_until: None,
            },
            PlanNode {
                node_id: "stage-2".into(),
                node_type: PlanNodeType::Stage,
                parent_id: None,
                namespace: "pentest".into(),
                objective: "漏洞扫描".into(),
                capability: Some("scanner".into()),
                target: Some(target.into()),
                status: PlanNodeStatus::Pending,
                priority: 2,
                success_criteria: serde_json::json!({"vuln_found":">=1"}),
                budget: budget.clone(),
                attempt: 0,
                max_attempts: 3,
                observation_refs: vec![],
                valid_until: None,
            },
            PlanNode {
                node_id: "stage-3".into(),
                node_type: PlanNodeType::Stage,
                parent_id: None,
                namespace: "pentest".into(),
                objective: "验证与报告".into(),
                capability: None,
                target: None,
                status: PlanNodeStatus::Pending,
                priority: 3,
                success_criteria: serde_json::json!({"report":"complete"}),
                budget,
                attempt: 0,
                max_attempts: 3,
                observation_refs: vec![],
                valid_until: None,
            },
        ];
        TaskCoordinationGraph {
            plan_id: uuid::Uuid::new_v4().to_string(),
            run_id: run_id.into(),
            version: 1,
            objective: format!("安全测试: {}", target),
            nodes,
            edges: vec![
                PlanEdge {
                    from: "stage-1".into(),
                    to: "stage-2".into(),
                    edge_type: PlanEdgeType::Requires,
                    condition: serde_json::Value::Null,
                },
                PlanEdge {
                    from: "stage-2".into(),
                    to: "stage-3".into(),
                    edge_type: PlanEdgeType::Requires,
                    condition: serde_json::Value::Null,
                },
            ],
            global_budget: PlanBudget {
                max_actions: 30,
                max_tool_calls: 50,
                max_duration_seconds: 3600,
                max_tokens: 200_000,
                max_cost: 100.0,
                max_concurrency: 4,
            },
            memory_refs: vec![],
            created_at: Utc::now(),
        }
    }

    pub fn fallback_template(run_id: &str, target: &str) -> TaskCoordinationGraph {
        Self::plan_pentest(run_id, target)
    }
}
