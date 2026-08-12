//! HPM-02: Meta-Planner — creates initial TaskCoordinationGraph.
//! Supports LLM-based planning with deterministic template fallback.

use chrono::Utc;
use kairos_platform_ai_server::AiModelFactory;
use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::cognition::*;
use serde_json::{Value, json};

/// System prompt for the LLM meta-planner. Describes the TCG format, available
/// capabilities, and the planning rules the model must follow.
const META_PLANNER_SYSTEM_PROMPT: &str = r#"你是渗透测试任务规划专家。根据目标信息和授权范围，生成一个任务协调图（TCG）。

## 输出方式
你必须调用 emit_plan 工具提交规划。不要输出其他文本。

## 阶段展开规则
你生成的每个 stage 的 objective 中文描述决定了下游自动化工具展开：

| objective 关键词 | 展开的工具动作 |
|---|---|
| 包含"侦察" | tcp_scan（端口扫描）、http_probe（HTTP探测）、dns_lookup（DNS查询） |
| 包含"扫描" | scanner_nmap（Nmap漏洞扫描）、scanner_nuclei（Nuclei模板扫描） |
| 包含"指纹"或"识别" | http_probe（HTTP探测）、web_fingerprint（Web指纹） |
| 包含"报告" | 无工具动作，由引擎生成确定性报告 |

- 无关键词匹配的阶段默认展开为 http_request
- namespace 固定为 "pentest"
- 每个阶段必须指定一个 capability 提示：recon / vuln_scan / web_fingerprint / dir_fuzz / validation

## 规划规则
1. 至少 3 个阶段，最多 7 个
2. 最后一个阶段的 objective 必须包含"报告"关键词（如"验证与报告"）
3. 第一阶段必须包含"侦察"或"扫描"关键词
4. edges 表示顺序依赖（Requires 边），stage-N 完成才能执行 stage-N+1
5. 图必须是无环有向图（DAG）
6. 根据目标类型挑选合适的阶段（Web 应用侧重指纹/目录扫描，网络侧重端口/服务发现）
7. 所有操作必须在授权范围内
8. 不得规划爆破、漏洞利用、提权、横向移动等攻击行为
9. node_id 使用 "stage-1"、"stage-2" 等格式
10. priority 从 1 递增（1 最高优先级）
"#;

/// Lightweight plan structure returned by the LLM. The Rust side fills in
/// budget, version, and other TCG metadata.
#[derive(Debug, serde::Deserialize)]
struct LlmPlanOutput {
    objective: String,
    stages: Vec<LlmStage>,
    edges: Vec<LlmEdge>,
}

#[derive(Debug, serde::Deserialize)]
struct LlmStage {
    node_id: String,
    objective: String,
    #[serde(default)]
    capability: Option<String>,
    priority: i32,
}

#[derive(Debug, serde::Deserialize)]
struct LlmEdge {
    from: String,
    to: String,
}

pub struct MetaPlanner;

impl MetaPlanner {
    /// Deterministic 3-stage template. Also used as the last-resort fallback.
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
                capability: Some("recon".into()),
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
                capability: Some("vuln_scan".into()),
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

    /// Fallback template — deterministic plan used when LLM planning is
    /// unavailable or produces an invalid graph.
    pub fn fallback_template(run_id: &str, target: &str) -> TaskCoordinationGraph {
        Self::plan_pentest(run_id, target)
    }

    /// Attempt LLM-based plan generation. Returns the graph and the event
    /// source label (`"llm-planner"` on success, `"deterministic-template"`
    /// on fallback). This function never fails — it always returns a valid
    /// graph via the fallback path.
    pub async fn plan_with_llm(
        pool: &PgPool,
        run_id: &str,
        target: &str,
        objective: Option<&str>,
        scope_snapshot: &Value,
        tenant_id: i64,
    ) -> (TaskCoordinationGraph, &'static str) {
        match Self::try_llm_plan(pool, target, objective, scope_snapshot, tenant_id).await {
            Ok(mut graph) => {
                graph.run_id = run_id.into();
                graph.plan_id = uuid::Uuid::new_v4().to_string();
                graph.version = 1;
                graph.created_at = Utc::now();
                (graph, "llm-planner")
            }
            Err(error) => {
                tracing::warn!(
                    run_id = %run_id,
                    error = %error,
                    "LLM meta-planner failed; falling back to deterministic template"
                );
                (
                    Self::fallback_template(run_id, target),
                    "deterministic-template",
                )
            }
        }
    }

    /// Core LLM call: resolve planner model → prompt → parse → validate.
    async fn try_llm_plan(
        pool: &PgPool,
        target: &str,
        objective: Option<&str>,
        scope_snapshot: &Value,
        tenant_id: i64,
    ) -> anyhow::Result<TaskCoordinationGraph> {
        let (model_id, temperature, max_tokens) =
            Self::resolve_planner_model(pool, tenant_id).await?;

        let factory = AiModelFactory::new(pool.clone());
        let tools = vec![Self::emit_plan_tool_schema()];

        let scope_json = serde_json::to_string_pretty(scope_snapshot)?;
        let user_prompt = format!(
            "## 测试目标\n- 目标地址：{target}\n- 任务说明：{}\n\
             - 授权范围：\n```json\n{scope_json}\n```\n\n请调用 emit_plan 输出任务协调图。",
            objective.unwrap_or("安全测试")
        );

        let messages = vec![
            json!({"role": "system", "content": META_PLANNER_SYSTEM_PROMPT}),
            json!({"role": "user", "content": user_prompt}),
        ];

        let response = factory
            .chat_tools(
                model_id,
                messages,
                tools,
                Some(temperature),
                Some(max_tokens),
            )
            .await
            .map_err(|e| anyhow::anyhow!("LLM planner call failed: {e:#?}"))?;

        Self::parse_tcg_from_response(&response, target)
    }

    /// Build the `emit_plan` tool schema the LLM must call to submit its plan.
    fn emit_plan_tool_schema() -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "emit_plan",
                "description": "提交渗透测试任务协调图（TCG）。包含按阶段组织的测试步骤和依赖关系。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "objective": {
                            "type": "string",
                            "description": "整体测试目标描述"
                        },
                        "stages": {
                            "type": "array",
                            "description": "测试阶段列表，按执行顺序排列",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "node_id": {
                                        "type": "string",
                                        "description": "阶段唯一ID，如 stage-1"
                                    },
                                    "objective": {
                                        "type": "string",
                                        "description": "阶段目标中文描述。必须包含侦察/扫描/指纹/识别/报告等关键词以触发正确的工具展开"
                                    },
                                    "capability": {
                                        "type": "string",
                                        "description": "所需能力：recon / vuln_scan / web_fingerprint / dir_fuzz / validation"
                                    },
                                    "priority": {
                                        "type": "integer",
                                        "description": "执行优先级，1 最高，逐阶段递增"
                                    }
                                },
                                "required": ["node_id", "objective", "priority"]
                            }
                        },
                        "edges": {
                            "type": "array",
                            "description": "阶段之间的依赖关系（Requires 边），stage-N → stage-N+1",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "from": {"type": "string", "description": "前置阶段 node_id"},
                                    "to": {"type": "string", "description": "后置阶段 node_id"}
                                },
                                "required": ["from", "to"]
                            }
                        }
                    },
                    "required": ["objective", "stages", "edges"]
                }
            }
        })
    }

    /// Extract the `emit_plan` tool call arguments from the LLM response and
    /// construct a validated [`TaskCoordinationGraph`].
    fn parse_tcg_from_response(
        response: &Value,
        target: &str,
    ) -> anyhow::Result<TaskCoordinationGraph> {
        let plan_args = response
            .pointer("/choices/0/message/tool_calls")
            .and_then(|v| v.as_array())
            .and_then(|calls| {
                calls.iter().find_map(|tc| {
                    let name = tc.pointer("/function/name")?.as_str()?;
                    if name == "emit_plan" {
                        tc.pointer("/function/arguments")?
                            .as_str()
                            .map(String::from)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| anyhow::anyhow!("LLM did not call emit_plan tool"))?;

        let plan: LlmPlanOutput = serde_json::from_str(&plan_args).map_err(|e| {
            anyhow::anyhow!("Failed to parse TCG JSON from LLM: {e}. Raw: {plan_args}")
        })?;

        if plan.stages.is_empty() {
            anyhow::bail!("LLM plan has no stages");
        }
        if plan.stages.len() > 10 {
            anyhow::bail!("LLM plan has too many stages: {}", plan.stages.len());
        }

        // Validate node_id uniqueness
        let ids: std::collections::HashSet<_> = plan.stages.iter().map(|s| &s.node_id).collect();
        if ids.len() != plan.stages.len() {
            anyhow::bail!("Duplicate stage node_ids in LLM plan");
        }

        // Validate all edge references exist
        for edge in &plan.edges {
            if !ids.contains(&edge.from) || !ids.contains(&edge.to) {
                anyhow::bail!("Edge references missing node: {} -> {}", edge.from, edge.to);
            }
        }

        let budget = PlanBudget {
            max_actions: 30,
            max_tool_calls: 50,
            max_duration_seconds: 3600,
            max_tokens: 200_000,
            max_cost: 100.0,
            max_concurrency: 4,
        };

        let nodes: Vec<PlanNode> = plan
            .stages
            .into_iter()
            .map(|s| PlanNode {
                node_id: s.node_id,
                node_type: PlanNodeType::Stage,
                parent_id: None,
                namespace: "pentest".into(),
                objective: s.objective,
                capability: s.capability,
                target: Some(target.into()),
                status: PlanNodeStatus::Pending,
                priority: s.priority,
                success_criteria: json!({"completed": true}),
                budget: budget.clone(),
                attempt: 0,
                max_attempts: 3,
                observation_refs: vec![],
                valid_until: None,
            })
            .collect();

        let edges: Vec<PlanEdge> = plan
            .edges
            .into_iter()
            .map(|e| PlanEdge {
                from: e.from,
                to: e.to,
                edge_type: PlanEdgeType::Requires,
                condition: Value::Null,
            })
            .collect();

        Ok(TaskCoordinationGraph {
            plan_id: String::new(), // filled in by caller
            run_id: String::new(),  // filled in by caller
            version: 0,             // filled in by caller
            objective: plan.objective,
            nodes,
            edges,
            global_budget: budget,
            memory_refs: vec![],
            created_at: Utc::now(),
        })
    }

    /// Resolve the default enabled chat model configured in the AI console.
    async fn resolve_planner_model(
        pool: &PgPool,
        _tenant_id: i64,
    ) -> anyhow::Result<(i64, f64, u32)> {
        let id: i64 = sqlx::query_scalar(
            "SELECT id FROM ai.model_configs
             WHERE type = 'chat' AND status = 0
             ORDER BY id LIMIT 1",
        )
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("未配置可用的聊天模型"))?;
        Ok((id, 0.7, 4096))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_template_is_valid() {
        let graph = MetaPlanner::plan_pentest("test-run", "192.168.1.1");
        kairos_platform_pent_core::cognition::validate_graph(&graph)
            .expect("deterministic template must be valid");
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        // Last stage must be reporting (contains "报告")
        let last = graph.nodes.last().unwrap();
        assert!(
            last.objective.contains("报告"),
            "last stage must be reporting"
        );
    }

    #[test]
    fn fallback_template_is_valid() {
        let graph = MetaPlanner::fallback_template("test-run", "example.com");
        kairos_platform_pent_core::cognition::validate_graph(&graph)
            .expect("fallback template must be valid");
    }

    #[test]
    fn emit_plan_tool_schema_is_valid_json() {
        let schema = MetaPlanner::emit_plan_tool_schema();
        // Must be a valid function tool definition
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], "emit_plan");
        let params = &schema["function"]["parameters"];
        assert_eq!(params["type"], "object");
        assert!(
            params["required"]
                .as_array()
                .is_some_and(|r| r.contains(&json!("stages")))
        );
    }

    #[test]
    fn parse_rejects_empty_stages() {
        let response = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "emit_plan",
                            "arguments": "{\"objective\":\"test\",\"stages\":[],\"edges\":[]}"
                        }
                    }]
                }
            }]
        });
        let err = MetaPlanner::parse_tcg_from_response(&response, "target")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("no stages"),
            "should reject empty stages: {err}"
        );
    }

    #[test]
    fn parse_rejects_missing_emit_plan_call() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "I will not call the tool"
                }
            }]
        });
        assert!(MetaPlanner::parse_tcg_from_response(&response, "target").is_err());
    }

    #[test]
    fn parse_rejects_duplicate_node_ids() {
        let response = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "emit_plan",
                            "arguments": "{\"objective\":\"t\",\"stages\":[{\"node_id\":\"s1\",\"objective\":\"资产侦察\",\"priority\":1},{\"node_id\":\"s1\",\"objective\":\"漏洞扫描\",\"priority\":2}],\"edges\":[]}"
                        }
                    }]
                }
            }]
        });
        let err = MetaPlanner::parse_tcg_from_response(&response, "target")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Duplicate"), "should reject dupes: {err}");
    }

    #[test]
    fn parse_rejects_edge_to_missing_node() {
        let response = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "emit_plan",
                            "arguments": "{\"objective\":\"t\",\"stages\":[{\"node_id\":\"s1\",\"objective\":\"资产侦察\",\"priority\":1}],\"edges\":[{\"from\":\"s1\",\"to\":\"s99\"}]}"
                        }
                    }]
                }
            }]
        });
        let err = MetaPlanner::parse_tcg_from_response(&response, "target")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("missing node"),
            "should reject dangling edge: {err}"
        );
    }

    #[test]
    fn parse_valid_plan_succeeds() {
        let response = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "emit_plan",
                            "arguments": "{\"objective\":\"对 example.com 进行Web安全测试\",\"stages\":[{\"node_id\":\"stage-1\",\"objective\":\"资产侦察\",\"capability\":\"recon\",\"priority\":1},{\"node_id\":\"stage-2\",\"objective\":\"漏洞扫描\",\"capability\":\"vuln_scan\",\"priority\":2},{\"node_id\":\"stage-3\",\"objective\":\"验证与报告\",\"capability\":\"validation\",\"priority\":3}],\"edges\":[{\"from\":\"stage-1\",\"to\":\"stage-2\"},{\"from\":\"stage-2\",\"to\":\"stage-3\"}]}"
                        }
                    }]
                }
            }]
        });
        let graph = MetaPlanner::parse_tcg_from_response(&response, "example.com")
            .expect("valid plan must parse");
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.objective, "对 example.com 进行Web安全测试");
        assert!(graph.nodes.iter().all(|n| n.namespace == "pentest"));
        // Validate through pent-core
        kairos_platform_pent_core::cognition::validate_graph(&graph)
            .expect("parsed graph must be valid");
    }
}
