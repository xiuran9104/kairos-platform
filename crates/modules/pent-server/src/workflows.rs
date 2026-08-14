//! Workflow management — CRUD over `pent_workflows` plus pure-function
//! graph validation and dry-run simulation.
//!
//! Graph shape: `{nodes:[{id, type, label?, config?}], edges:[{id?, source, target, config?{branch?}}]}`
//! with node types `start|tool|agent|condition|hitl|output|end`.

use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/workflows", get(list_workflows).post(create_workflow))
        .route(
            "/pent/workflows/{id}",
            get(get_workflow).put(update_workflow).delete(delete_workflow),
        )
        .route("/pent/workflows/validate", post(validate_handler))
        .route("/pent/workflows/dry-run", post(dry_run_handler))
        .route("/pent/workflows/execute", post(execute_handler))
}

/// Workflow ids follow `^[a-z0-9][a-z0-9-]{0,99}$`.
pub(crate) fn valid_workflow_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    id.len() <= 100
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn node_id(node: &Value) -> &str {
    node.get("id").and_then(Value::as_str).unwrap_or("")
}

fn node_type(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

fn edge_endpoint<'a>(edge: &'a Value, key: &str) -> &'a str {
    edge.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Validate a workflow graph, returning human-readable errors (empty = valid).
/// Pure function so it can be unit-tested without a database.
pub(crate) fn validate_graph(graph: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let (Some(nodes), Some(edges)) = (
        graph.get("nodes").and_then(Value::as_array),
        graph.get("edges").and_then(Value::as_array),
    ) else {
        return vec!["graph 必须是包含 nodes 与 edges 数组的对象".to_string()];
    };

    let mut ids = HashSet::new();
    for node in nodes {
        let id = node_id(node);
        if id.is_empty() {
            errors.push("存在缺少 id 的节点".to_string());
        } else if !ids.insert(id.to_string()) {
            errors.push(format!("节点 id 重复：{id}"));
        }
    }

    let start_count = nodes.iter().filter(|n| node_type(n) == "start").count();
    if start_count != 1 {
        errors.push(format!("必须恰好 1 个 start 节点，当前 {start_count} 个"));
    }
    if !nodes.iter().any(|n| node_type(n) == "output") {
        errors.push("至少需要 1 个 output 节点".to_string());
    }

    let mut indeg: HashMap<&str, usize> = HashMap::new();
    let mut outdeg: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        let source = edge_endpoint(edge, "source");
        let target = edge_endpoint(edge, "target");
        if source.is_empty() || target.is_empty() {
            errors.push("存在缺少 source 或 target 的边".to_string());
            continue;
        }
        if source == target {
            errors.push(format!("存在自环边：{source}"));
        }
        if !ids.contains(source) {
            errors.push(format!("边引用了不存在的节点：{source}"));
        }
        if !ids.contains(target) {
            errors.push(format!("边引用了不存在的节点：{target}"));
        }
        *indeg.entry(target).or_default() += 1;
        *outdeg.entry(source).or_default() += 1;
        adjacency.entry(source).or_default().push(target);
    }

    for node in nodes {
        let id = node_id(node);
        if id.is_empty() {
            continue;
        }
        let ty = node_type(node);
        let in_count = indeg.get(id).copied().unwrap_or(0);
        let out_count = outdeg.get(id).copied().unwrap_or(0);
        if ty == "start" && in_count > 0 {
            errors.push(format!("start 节点 {id} 不能有入边"));
        }
        if matches!(ty, "output" | "end") && out_count > 0 {
            errors.push(format!("{ty} 节点 {id} 不能有出边"));
        }
        if ty != "start" && in_count == 0 {
            errors.push(format!("节点 {id} 缺少入边"));
        }
        if !matches!(ty, "output" | "end") && out_count == 0 {
            errors.push(format!("节点 {id} 缺少出边"));
        }
        let config = node.get("config").cloned().unwrap_or_else(|| json!({}));
        match ty {
            "tool" => {
                let tool_name = config
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if tool_name.is_empty() {
                    errors.push(format!("tool 节点 {id} 缺少 config.tool_name"));
                }
            }
            "condition" => {
                let expression = config
                    .get("expression")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if expression.is_empty() {
                    errors.push(format!("condition 节点 {id} 缺少 config.expression"));
                }
                if out_count > 2 {
                    errors.push(format!("condition 节点 {id} 出边数量必须为 1-2 条，当前 {out_count} 条"));
                }
                let mut branches = HashSet::new();
                for edge in edges.iter().filter(|e| edge_endpoint(e, "source") == id) {
                    let branch = edge
                        .get("config")
                        .and_then(|c| c.get("branch"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !branch.is_empty() && !branches.insert(branch.to_string()) {
                        errors.push(format!("condition 节点 {id} 的分支标签重复：{branch}"));
                    }
                }
            }
            "output" => {
                let output_key = config
                    .get("output_key")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if output_key.is_empty() {
                    errors.push(format!("output 节点 {id} 缺少 config.output_key"));
                }
            }
            _ => {}
        }
    }

    // Cycle detection via iterative DFS with white/gray/black coloring.
    let mut color: HashMap<&str, u8> = HashMap::new(); // 1=gray, 2=black
    let mut has_cycle = false;
    for id in &ids {
        if color.contains_key(id.as_str()) {
            continue;
        }
        let mut stack = vec![(id.as_str(), false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                color.insert(current, 2);
                continue;
            }
            match color.get(current).copied().unwrap_or(0) {
                1 => {
                    has_cycle = true;
                    continue;
                }
                2 => continue,
                _ => {}
            }
            color.insert(current, 1);
            stack.push((current, true));
            if let Some(next) = adjacency.get(current) {
                for target in next {
                    stack.push((target, false));
                }
            }
        }
    }
    if has_cycle {
        errors.push("图中存在环".to_string());
    }

    errors
}

/// Simulate a dry run from the start node, following the first out-edge at
/// every step. Returns `{trace:[...]}` or `{trace:[], errors:[...]}`.
pub(crate) fn simulate_graph(graph: &Value, inputs: &Value) -> Value {
    let errors = validate_graph(graph);
    if !errors.is_empty() {
        return json!({"trace": [], "errors": errors});
    }
    let nodes = graph["nodes"].as_array().cloned().unwrap_or_default();
    let edges = graph["edges"].as_array().cloned().unwrap_or_default();
    let by_id: HashMap<&str, &Value> = nodes.iter().map(|n| (node_id(n), n)).collect();
    let message = inputs
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("");

    let first_out_edge = |id: &str| -> Option<&Value> {
        edges.iter().find(|e| edge_endpoint(e, "source") == id)
    };

    let mut trace = Vec::new();
    let mut visited = HashSet::new();
    let mut current = nodes.iter().find(|n| node_type(n) == "start").cloned();
    while let Some(node) = current {
        let id = node_id(&node).to_string();
        if !visited.insert(id.clone()) {
            break;
        }
        let ty = node_type(&node).to_string();
        let label = node
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let config = node.get("config").cloned().unwrap_or_else(|| json!({}));
        let next_edge = first_out_edge(&id);
        let detail = match ty.as_str() {
            "start" => "工作流开始".to_string(),
            "tool" => {
                let tool_name = config
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                format!("模拟调用工具 {tool_name}")
            }
            "agent" => format!("模拟 Agent 执行：{label}"),
            "condition" => {
                let expression = config
                    .get("expression")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let branch = next_edge
                    .and_then(|e| e.get("config"))
                    .and_then(|c| c.get("branch"))
                    .and_then(Value::as_str)
                    .unwrap_or("default");
                format!("模拟条件 [{expression}]，选择分支 {branch}")
            }
            "hitl" => "模拟人工确认，自动通过".to_string(),
            "output" => {
                let output_key = config
                    .get("output_key")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                format!("输出 {output_key} = {message}")
            }
            "end" => "工作流结束".to_string(),
            other => format!("模拟节点 {other}"),
        };
        trace.push(json!({
            "node_id": id,
            "label": label,
            "type": ty,
            "status": "simulated",
            "detail": detail,
        }));
        if matches!(ty.as_str(), "output" | "end") {
            break;
        }
        current = next_edge.and_then(|e| by_id.get(edge_endpoint(e, "target")).copied().cloned());
    }
    json!({"trace": trace})
}

async fn list_workflows(
    State(s): State<PentState>,
    Query(params): Query<HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let include_disabled = params
        .get("include_disabled")
        .is_some_and(|v| v == "true" || v == "1");
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_workflows t
         WHERE t.tenant_id=$1 AND ($2 OR t.enabled)
         ORDER BY t.created_at DESC",
    )
    .bind(tenant_id)
    .bind(include_disabled)
    .fetch_all(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn create_workflow(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let id = body
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::bad_request("id 不能为空"))?;
    if !valid_workflow_id(id) {
        return Err(AppError::bad_request(
            "id 必须匹配 ^[a-z0-9][a-z0-9-]{0,99}$",
        ));
    }
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::bad_request("name 不能为空"))?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pent_workflows WHERE id=$1 AND tenant_id=$2)",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    if exists {
        return Err(AppError::bad_request(format!("工作流 id 已存在：{id}")));
    }
    sqlx::query(
        "INSERT INTO pent_workflows(id, tenant_id, name, description, enabled, graph)
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(name)
    .bind(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )
    .bind(body.get("enabled").and_then(Value::as_bool).unwrap_or(true))
    .bind(
        body.get("graph")
            .cloned()
            .unwrap_or_else(|| json!({"nodes": [], "edges": []})),
    )
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?;
    get_workflow(State(s), Path(id.to_string()), user).await
}

async fn get_workflow(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_workflows t WHERE t.id=$1 AND t.tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .ok_or_else(|| AppError::not_found("工作流不存在"))?;
    Ok(Json(ApiResponse::new(row)))
}

async fn update_workflow(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        if name.trim().is_empty() {
            return Err(AppError::bad_request("name 不能为空"));
        }
    }
    if let Some(graph) = body.get("graph") {
        if !graph.is_object() {
            return Err(AppError::bad_request("graph 必须是对象"));
        }
    }
    let graph = body.get("graph").cloned();
    let affected = sqlx::query(
        "UPDATE pent_workflows SET
             name=COALESCE($3, name),
             description=COALESCE($4, description),
             enabled=COALESCE($5, enabled),
             graph=COALESCE($6, graph),
             version=version + CASE WHEN $6 IS NULL THEN 0 ELSE 1 END,
             updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("description").and_then(Value::as_str))
    .bind(body.get("enabled").and_then(Value::as_bool))
    .bind(graph)
    .execute(&s.pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("工作流不存在"));
    }
    get_workflow(State(s), Path(id), user).await
}

async fn delete_workflow(
    State(s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let tenant_id = security::tenant_id(&user)?;
    let affected = sqlx::query("DELETE FROM pent_workflows WHERE id=$1 AND tenant_id=$2")
        .bind(&id)
        .bind(tenant_id)
        .execute(&s.pool)
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("工作流不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn validate_handler(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let graph = body
        .get("graph")
        .ok_or_else(|| AppError::bad_request("graph 不能为空"))?;
    let errors = validate_graph(graph);
    Ok(Json(ApiResponse::new(json!({
        "valid": errors.is_empty(),
        "errors": errors,
    }))))
}

async fn dry_run_handler(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let graph = body
        .get("graph")
        .ok_or_else(|| AppError::bad_request("graph 不能为空"))?;
    let inputs = body.get("inputs").cloned().unwrap_or_else(|| json!({}));
    Ok(Json(ApiResponse::new(simulate_graph(graph, &inputs))))
}

/// Evaluate a `condition` node expression against the accumulated context.
/// Supported forms: `true`/`false`, `success`, `success == true|false`,
/// `output contains "x"`, `output == "x"`, `last_tool == "name"`.
pub(crate) fn evaluate_condition(expression: &str, context: &Value) -> Result<bool, String> {
    let expr = expression.trim();
    let success = context.get("success").and_then(Value::as_bool).unwrap_or(false);
    let output = context.get("output").and_then(Value::as_str).unwrap_or("");
    let last_tool = context.get("last_tool").and_then(Value::as_str).unwrap_or("");
    let needle = |rest: &str| -> String {
        rest.trim().trim_matches('"').trim_matches('\'').to_string()
    };
    match expr {
        "true" => Ok(true),
        "false" => Ok(false),
        "success" => Ok(success),
        "success == true" | "success == 1" => Ok(success),
        "success == false" | "success == 0" => Ok(!success),
        _ => {
            if let Some(rest) = expr.strip_prefix("output contains ") {
                return Ok(output.contains(&needle(rest)));
            }
            if let Some(rest) = expr.strip_prefix("output == ") {
                return Ok(output == needle(rest));
            }
            if let Some(rest) = expr.strip_prefix("output != ") {
                return Ok(output != needle(rest));
            }
            if let Some(rest) = expr.strip_prefix("last_tool == ") {
                return Ok(last_tool == needle(rest));
            }
            Err(format!("不支持的表达式: {expr}"))
        }
    }
}

/// Execute a workflow graph for real: tool nodes run through the sandboxed
/// executor, condition nodes branch on accumulated context, output nodes
/// collect results. `agent`/`hitl` nodes are recorded but not yet executed by
/// the conversation runtime. Returns `{trace, outputs, status}`.
pub(crate) async fn execute_workflow_graph(
    graph: &Value,
    inputs: &Value,
    pool: &kairos_platform_framework_database::PgPool,
) -> Value {
    let errors = validate_graph(graph);
    if !errors.is_empty() {
        return json!({"trace": [], "outputs": {}, "errors": errors, "status": "invalid"});
    }
    let target = inputs.get("target").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let nodes = graph["nodes"].as_array().cloned().unwrap_or_default();
    let edges = graph["edges"].as_array().cloned().unwrap_or_default();
    let by_id: HashMap<String, Value> = nodes
        .iter()
        .map(|n| (node_id(n).to_string(), n.clone()))
        .collect();
    let out_edges = |id: &str| {
        edges
            .iter()
            .filter(|e| edge_endpoint(e, "source") == id)
            .cloned()
            .collect::<Vec<_>>()
    };

    let mut context = json!({"target": target, "success": false, "output": "", "last_tool": ""});
    let mut outputs = serde_json::Map::new();
    let mut trace: Vec<Value> = Vec::new();
    let mut visited = HashSet::new();
    let mut status = "completed";
    let mut current = nodes.iter().find(|n| node_type(n) == "start").cloned();

    while let Some(node) = current {
        let id = node_id(&node).to_string();
        if !visited.insert(id.clone()) {
            status = "looped";
            break;
        }
        let ty = node_type(&node).to_string();
        let label = node
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let config = node.get("config").cloned().unwrap_or_else(|| json!({}));

        let mut next: Option<String> = None;
        let mut detail = String::new();
        let mut node_status = "completed";

        match ty.as_str() {
            "start" => {
                detail = "工作流开始".to_string();
            }
            "tool" => {
                let tool_name = config
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let node_target = config
                    .get("target")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .or_else(|| {
                        context
                            .get("target")
                            .and_then(Value::as_str)
                            .filter(|v| !v.is_empty())
                            .map(str::to_owned)
                    });
                match node_target {
                    Some(target) => {
                        let limits = crate::infrastructure::process::ToolExecutionLimits {
                            timeout_seconds: 300,
                            network_access: crate::infrastructure::process::NetworkPolicy::TargetOnly(
                                target.clone(),
                            ),
                            ..Default::default()
                        };
                        match crate::infrastructure::process::execute_tool(
                            &tool_name,
                            &[target.clone()],
                            &limits,
                        )
                        .await
                        {
                            Ok(output) => {
                                let stdout = output.stdout;
                                context["success"] = json!(output.exit_code == 0);
                                context["last_tool"] = json!(tool_name);
                                context["output"] = json!(stdout.chars().take(2_000).collect::<String>());
                                detail = format!(
                                    "执行 {tool_name} 完成（exit {}）",
                                    output.exit_code
                                );
                                if output.exit_code != 0 {
                                    node_status = "failed";
                                }
                            }
                            Err(error) => {
                                context["success"] = json!(false);
                                context["last_tool"] = json!(tool_name);
                                context["output"] = json!("");
                                detail = format!("执行 {tool_name} 失败: {error}");
                                node_status = "failed";
                            }
                        }
                    }
                    None => {
                        detail = format!("工具 {tool_name} 缺少目标，跳过");
                        node_status = "skipped";
                    }
                }
            }
            "agent" => {
                detail = format!("Agent 节点 {label} 需由对话运行时执行，本次未接入");
                node_status = "skipped";
            }
            "condition" => {
                let expression = config
                    .get("expression")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match evaluate_condition(expression, &context) {
                    Ok(decision) => {
                        detail = format!("条件 [{expression}] → {decision}");
                        let branch = if decision { "true" } else { "false" };
                        let node_edges = out_edges(&id);
                        next = node_edges
                            .iter()
                            .find(|e| {
                                e.get("config")
                                    .and_then(|c| c.get("branch"))
                                    .and_then(Value::as_str)
                                    == Some(branch)
                            })
                            .or_else(|| {
                                node_edges.iter().find(|e| {
                                    e.get("config").and_then(|c| c.get("branch")).is_none()
                                })
                            })
                            .map(|e| edge_endpoint(e, "target").to_string());
                    }
                    Err(error) => {
                        detail = format!("条件求值失败: {error}");
                        node_status = "failed";
                    }
                }
            }
            "hitl" => {
                detail = "人工确认节点（本次执行记录为待确认）".to_string();
                node_status = "pending";
            }
            "output" => {
                let output_key = config
                    .get("output_key")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                outputs.insert(
                    output_key.to_string(),
                    context.get("output").cloned().unwrap_or(Value::Null),
                );
                detail = format!("输出 {output_key}");
            }
            "end" => {
                detail = "工作流结束".to_string();
            }
            other => {
                detail = format!("节点类型 {other}");
            }
        }

        trace.push(json!({
            "node_id": id,
            "label": label,
            "type": ty,
            "status": node_status,
            "detail": detail,
        }));

        if matches!(ty.as_str(), "output" | "end") {
            break;
        }
        if node_status == "failed" {
            status = "failed";
            break;
        }
        if let Some(target_id) = next {
            current = by_id.get(&target_id).cloned();
        } else {
            current = out_edges(&id)
                .first()
                .and_then(|e| by_id.get(edge_endpoint(e, "target")).cloned());
        }
    }

    let _ = pool;
    json!({
        "trace": trace,
        "outputs": outputs,
        "status": status,
    })
}

async fn execute_handler(
    State(s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let graph = body
        .get("graph")
        .ok_or_else(|| AppError::bad_request("graph 不能为空"))?;
    let inputs = body.get("inputs").cloned().unwrap_or_else(|| json!({}));
    Ok(Json(ApiResponse::new(
        execute_workflow_graph(graph, &inputs, &s.pool).await,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_graph() -> Value {
        json!({
            "nodes": [
                {"id": "s", "type": "start"},
                {"id": "t", "type": "tool", "label": "扫描", "config": {"tool_name": "nmap"}},
                {"id": "o", "type": "output", "config": {"output_key": "result"}}
            ],
            "edges": [
                {"source": "s", "target": "t"},
                {"source": "t", "target": "o"}
            ]
        })
    }

    #[test]
    fn valid_graph_passes() {
        assert!(validate_graph(&base_graph()).is_empty());
    }

    #[test]
    fn missing_start_fails() {
        let mut graph = base_graph();
        graph["nodes"][0]["type"] = json!("agent");
        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| e.contains("start 节点")));
    }

    #[test]
    fn cycle_is_detected() {
        let graph = json!({
            "nodes": [
                {"id": "s", "type": "start"},
                {"id": "a", "type": "tool", "config": {"tool_name": "nmap"}},
                {"id": "b", "type": "tool", "config": {"tool_name": "nikto"}},
                {"id": "o", "type": "output", "config": {"output_key": "r"}}
            ],
            "edges": [
                {"source": "s", "target": "a"},
                {"source": "a", "target": "b"},
                {"source": "b", "target": "a"},
                {"source": "b", "target": "o"}
            ]
        });
        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| e.contains("环")));
    }

    #[test]
    fn duplicate_condition_branch_fails() {
        let graph = json!({
            "nodes": [
                {"id": "s", "type": "start"},
                {"id": "c", "type": "condition", "config": {"expression": "score > 5"}},
                {"id": "o1", "type": "output", "config": {"output_key": "a"}},
                {"id": "o2", "type": "output", "config": {"output_key": "b"}}
            ],
            "edges": [
                {"source": "s", "target": "c"},
                {"source": "c", "target": "o1", "config": {"branch": "yes"}},
                {"source": "c", "target": "o2", "config": {"branch": "yes"}}
            ]
        });
        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| e.contains("分支标签重复")));
    }

    #[test]
    fn tool_without_tool_name_fails() {
        let mut graph = base_graph();
        graph["nodes"][1]["config"] = json!({});
        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| e.contains("tool_name")));
    }

    #[test]
    fn edge_referencing_missing_node_fails() {
        let mut graph = base_graph();
        graph["edges"][0]["target"] = json!("ghost");
        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| e.contains("不存在的节点")));
    }

    #[test]
    fn dry_run_produces_simulated_trace() {
        let result = simulate_graph(&base_graph(), &json!({"message": "hello"}));
        let trace = result["trace"].as_array().expect("trace");
        assert_eq!(trace.len(), 3);
        assert!(trace.iter().all(|t| t["status"] == "simulated"));
        assert_eq!(trace[1]["detail"], "模拟调用工具 nmap");
        assert_eq!(trace[2]["detail"], "输出 result = hello");
    }

    #[test]
    fn dry_run_returns_errors_for_invalid_graph() {
        let result = simulate_graph(&json!({"nodes": [], "edges": []}), &json!({}));
        assert_eq!(result["trace"].as_array().unwrap().len(), 0);
        assert!(!result["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn workflow_id_pattern() {
        assert!(valid_workflow_id("a"));
        assert!(valid_workflow_id("scan-01"));
        assert!(!valid_workflow_id(""));
        assert!(!valid_workflow_id("-bad"));
        assert!(!valid_workflow_id("Bad"));
        assert!(!valid_workflow_id(&"a".repeat(101)));
    }

    #[test]
    fn condition_evaluates_supported_forms() {
        let ctx = json!({"success": true, "output": "Nmap scan report", "last_tool": "nmap"});
        assert_eq!(evaluate_condition("success == true", &ctx).unwrap(), true);
        assert_eq!(evaluate_condition("success == false", &ctx).unwrap(), false);
        assert!(evaluate_condition("output contains \"Nmap\"", &ctx).unwrap());
        assert!(!evaluate_condition("output contains \"nginx\"", &ctx).unwrap());
        assert!(evaluate_condition("output == \"Nmap scan report\"", &ctx).unwrap());
        assert!(evaluate_condition("last_tool == \"nmap\"", &ctx).unwrap());
        assert!(evaluate_condition("true", &ctx).unwrap());
        assert!(evaluate_condition("bogus expr", &ctx).is_err());
    }
}
