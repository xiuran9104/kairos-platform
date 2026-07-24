use std::collections::{HashMap, HashSet};

use kairos_platform_pent_api::{
    PlanDelta, PlanDeltaOperation, PlanNodeStatus, TaskCoordinationGraph,
};

pub fn validate_graph(graph: &TaskCoordinationGraph) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let ids: HashSet<_> = graph
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    if ids.len() != graph.nodes.len() {
        errors.push("duplicate_node_id".into());
    }
    for edge in &graph.edges {
        if !ids.contains(edge.from.as_str()) || !ids.contains(edge.to.as_str()) {
            errors.push(format!(
                "edge_references_missing_node:{}->{}",
                edge.from, edge.to
            ));
        }
    }
    if has_cycle(graph) {
        errors.push("graph_contains_cycle".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn apply_plan_delta(
    graph: &TaskCoordinationGraph,
    delta: &PlanDelta,
) -> Result<TaskCoordinationGraph, Vec<String>> {
    if graph.plan_id != delta.plan_id || graph.version != delta.base_version {
        return Err(vec!["plan_version_conflict".into()]);
    }
    let mut next = graph.clone();
    for operation in &delta.operations {
        match operation {
            PlanDeltaOperation::AddNode { node } => next.nodes.push(node.clone()),
            PlanDeltaOperation::RemoveNode { node_id } => {
                next.nodes.retain(|node| &node.node_id != node_id);
                next.edges
                    .retain(|edge| &edge.from != node_id && &edge.to != node_id);
            }
            PlanDeltaOperation::AddEdge { edge } => next.edges.push(edge.clone()),
            PlanDeltaOperation::RemoveEdge { from, to } => next
                .edges
                .retain(|edge| &edge.from != from || &edge.to != to),
            PlanDeltaOperation::MarkStale { node_id, .. } => {
                if let Some(node) = next.nodes.iter_mut().find(|node| &node.node_id == node_id) {
                    node.status = PlanNodeStatus::Stale;
                }
            }
            PlanDeltaOperation::UpdateNode { node_id, patch } => {
                let Some(node) = next.nodes.iter_mut().find(|node| &node.node_id == node_id) else {
                    return Err(vec![format!("node_not_found:{node_id}")]);
                };
                if let Some(priority) = patch.get("priority").and_then(|value| value.as_i64()) {
                    node.priority = priority as i32;
                }
                if let Some(objective) = patch.get("objective").and_then(|value| value.as_str()) {
                    node.objective = objective.into();
                }
            }
        }
    }
    next.version += 1;
    validate_graph(&next)?;
    Ok(next)
}

fn has_cycle(graph: &TaskCoordinationGraph) -> bool {
    let mut indegree: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), 0))
        .collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        *indegree.entry(edge.to.as_str()).or_default() += 1;
        outgoing
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut ready: Vec<&str> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(node) = ready.pop() {
        visited += 1;
        for target in outgoing.get(node).into_iter().flatten() {
            let degree = indegree.get_mut(target).expect("target exists");
            *degree -= 1;
            if *degree == 0 {
                ready.push(target);
            }
        }
    }
    visited != graph.nodes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use kairos_platform_pent_api::{
        PlanBudget, PlanDelta, PlanEdge, PlanEdgeType, PlanNode, PlanNodeType, ReplanLevel,
    };

    #[test]
    fn rejects_cycle_and_stale_delta_version() {
        let mut graph = graph();
        graph.edges.push(PlanEdge {
            from: "b".into(),
            to: "a".into(),
            edge_type: PlanEdgeType::Requires,
            condition: serde_json::json!({}),
        });
        assert!(
            validate_graph(&graph)
                .unwrap_err()
                .contains(&"graph_contains_cycle".into())
        );

        graph.edges.pop();
        let delta = PlanDelta {
            delta_id: "d".into(),
            plan_id: graph.plan_id.clone(),
            base_version: 0,
            replan_level: ReplanLevel::Stage,
            reason_codes: vec![],
            operations: vec![],
            proposed_by: "test".into(),
            created_at: Utc::now(),
        };
        assert_eq!(
            apply_plan_delta(&graph, &delta).unwrap_err(),
            vec!["plan_version_conflict"]
        );
    }

    fn graph() -> TaskCoordinationGraph {
        let budget = PlanBudget {
            max_actions: 2,
            max_tool_calls: 2,
            max_duration_seconds: 60,
            max_tokens: 100,
            max_cost: 1.0,
            max_concurrency: 1,
        };
        TaskCoordinationGraph {
            plan_id: "p".into(),
            run_id: "r".into(),
            version: 1,
            objective: "test".into(),
            nodes: ["a", "b"]
                .into_iter()
                .map(|id| PlanNode {
                    node_id: id.into(),
                    node_type: PlanNodeType::Action,
                    parent_id: None,
                    namespace: "scan".into(),
                    objective: id.into(),
                    capability: None,
                    target: None,
                    status: PlanNodeStatus::Pending,
                    priority: 0,
                    success_criteria: serde_json::json!({}),
                    budget: budget.clone(),
                    attempt: 0,
                    max_attempts: 1,
                    observation_refs: vec![],
                    valid_until: None,
                })
                .collect(),
            edges: vec![PlanEdge {
                from: "a".into(),
                to: "b".into(),
                edge_type: PlanEdgeType::Requires,
                condition: serde_json::json!({}),
            }],
            global_budget: budget,
            memory_refs: vec![],
            created_at: Utc::now(),
        }
    }
}
