use kairos_platform_framework_database::PgPool;
use serde_json::{Value, json};

use super::tool_executor::has_conversation_handler;

/// FTS search on tool `embedding_document` using PostgreSQL full-text search.
/// Falls back to an empty result set on any error — callers must degrade
/// gracefully to loading the full catalog.
async fn tool_fts_search(pool: &PgPool, query: &str, top_k: usize) -> Vec<(String, f32)> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let result = sqlx::query_as::<_, (String, f64)>(
        "SELECT tool_id, ts_rank(
            to_tsvector('simple', coalesce(embedding_document, description)),
            plainto_tsquery('simple', $1)
        ) AS rank
         FROM pent_tool_definitions
         WHERE lifecycle_status = 'active'
           AND embedding_document IS NOT NULL
           AND embedding_document != ''
         ORDER BY rank DESC
         LIMIT $2",
    )
    .bind(query.chars().take(1000).collect::<String>())
    .bind(top_k as i64)
    .fetch_all(pool)
    .await;
    match result {
        Ok(rows) => rows
            .into_iter()
            .filter(|(_, rank)| *rank > 0.0)
            .map(|(id, rank)| (id, rank as f32))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "FTS tool search failed; falling back to full catalog");
            Vec::new()
        }
    }
}

/// Hybrid retrieval: FTS → vector (if embedding available) → merge.
/// Returns ranked tool IDs with retrieval scores. On any failure, returns
/// an empty list — the caller degrades to the full catalog.
async fn retrieve_tool_ids(pool: &PgPool, intent: &str, top_k: usize) -> Vec<(String, f32)> {
    let fts = tool_fts_search(pool, intent, top_k * 3).await;
    if fts.is_empty() {
        return Vec::new();
    }
    // Optional vector search: if embedding provider is configured, augment
    // with semantic similarity results.
    let vector_results = {
        let mut embedder = crate::infrastructure::embedding::EmbeddingProvider::new(pool.clone());
        if embedder.try_configure().await.unwrap_or(false) {
            if let Some(embedding) = embedder.embed(intent).await {
                let index = crate::infrastructure::vector::PgVectorIndex::new(pool.clone());
                match index.search(&embedding, None, top_k * 2).await {
                    Ok(results) => results
                        .into_iter()
                        .map(|r| (r.tool_id, r.score))
                        .collect::<Vec<_>>(),
                    Err(error) => {
                        tracing::warn!(%error, "vector search failed; using FTS only");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };

    // Merge: deduplicate, sort by score desc, truncate.
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::with_capacity(fts.len() + vector_results.len());
    for (id, score) in fts.into_iter().chain(vector_results) {
        if seen.insert(id.clone()) {
            merged.push((id, score));
        }
    }
    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(top_k);
    tracing::debug!(
        count = merged.len(),
        intent = %intent.chars().take(50).collect::<String>(),
        "hybrid tool retrieval complete"
    );
    merged
}

/// Load tool schemas from the maintained registry instead of duplicating them
/// in the conversation runtime.
pub async fn load_conversation_tool_definitions(pool: &PgPool) -> anyhow::Result<Vec<Value>> {
    let rows = sqlx::query_as::<_, (String, String, Value)>(
        "SELECT tool_id, description, input_schema
         FROM pent_tool_definitions
         WHERE lifecycle_status = 'active'
         ORDER BY
           CASE risk_level
             WHEN 'critical' THEN 1
             WHEN 'high' THEN 2
             WHEN 'medium' THEN 3
             WHEN 'low' THEN 4
             ELSE 5
           END,
           name",
    )
    .fetch_all(pool)
    .await?;

    let mut definitions = rows
        .into_iter()
        .filter(|(name, _, _)| has_conversation_handler(name))
        .map(|(name, description, parameters)| {
            json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": parameters
                }
            })
        })
        .collect::<Vec<_>>();

    // DesRed-style MCP-first extension point.  Server qualification keeps two
    // providers with the same tool name unambiguous for the model and routes
    // the call back to the selected MCP server without a second lookup.
    let registry = crate::tooling::mcp::registry::shared().await;
    let mcp_tools = registry.all_tools().await;
    let internal_recipe_names = mcp_tools
        .iter()
        .filter(|(server_id, _)| server_id == crate::tooling::mcp::internal::INTERNAL_SERVER_ID)
        .map(|(_, tool)| tool.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    // Once a host tool has a local MCP recipe, do not expose its legacy direct
    // function definition. This makes the migration opt-in per recipe while
    // retaining compatibility for tools that have not moved yet.
    definitions.retain(|definition| {
        definition
            .pointer("/function/name")
            .and_then(Value::as_str)
            .is_none_or(|name| !internal_recipe_names.contains(name))
    });
    for (server_id, tool) in mcp_tools {
        definitions.push(json!({
            "type": "function",
            "function": {
                "name": format!("mcp_{server_id}__{}", tool.name),
                "description": tool.description,
                "parameters": tool.input_schema,
            }
        }));
    }

    // This guarded validator is implemented by the conversation runtime but is
    // not yet part of older database snapshots.
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("file_upload_validate")
    }) {
        definitions.push(file_upload_validator_definition());
    }
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("knowledge_search")
    }) {
        definitions.push(knowledge_search_definition());
    }
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("vision_analyze")
    }) {
        definitions.push(vision_analyze_definition());
    }
    for definition in evidence_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }
    for definition in traffic_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }
    for definition in finding_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }
    for definition in skill_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }
    for definition in agent_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }
    for definition in super::plan_board::definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }

    Ok(definitions)
}

/// DesRed-style dynamic tool disclosure: keep control/evidence tools visible
/// and retrieve the remaining tools against the current user intent.
pub async fn load_tools_for_intent(
    pool: &PgPool,
    intent: &str,
    config: &Value,
) -> anyhow::Result<Vec<Value>> {
    let all = load_conversation_tool_definitions(pool).await?;
    if config.get("tool_search_enabled").and_then(Value::as_bool) == Some(false) {
        return Ok(all);
    }
    let threshold = std::env::var("PENTA_TOOL_SEARCH_MIN_TOOLS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .or_else(|| {
            config
                .get("tool_search_min_tools")
                .and_then(Value::as_u64)
                .map(|v| v as usize)
        })
        .unwrap_or(20);
    if all.len() < threshold {
        return Ok(all);
    }
    let top_k = std::env::var("PENTA_TOOL_SEARCH_TOP_K")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .or_else(|| {
            config
                .get("tool_search_top_k")
                .and_then(Value::as_u64)
                .map(|v| v as usize)
        })
        .unwrap_or(24)
        .clamp(8, 80);
    let retrieved: std::collections::HashSet<String> = retrieve_tool_ids(pool, intent, top_k)
        .await
        .into_iter()
        .map(|(id, _)| id.to_ascii_lowercase())
        .collect();
    if retrieved.is_empty() {
        return Ok(all);
    }
    const ALWAYS: &[&str] = &[
        "knowledge_search",
        "evidence_list",
        "evidence_search",
        "evidence_view",
        "traffic_list",
        "traffic_view",
        "skill_list",
        "skill_load",
        "agent_spawn",
        "agent_status",
        "agent_collect",
        "agent_list",
        "record_finding",
        "read_file",
        "glob",
        "grep",
    ];
    let selected = all
        .into_iter()
        .filter(|definition| {
            let name = definition
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let lower = name.to_ascii_lowercase();
            ALWAYS.contains(&lower.as_str())
                || lower.starts_with("mcp_")
                || retrieved.contains(&lower)
        })
        .collect::<Vec<_>>();
    tracing::debug!(
        available = selected.len(),
        retrieved = retrieved.len(),
        "dynamic tool disclosure applied"
    );
    Ok(selected)
}

fn evidence_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "evidence_list",
                "description": "列出当前 Run 已持久化的 Evidence 元数据，不返回完整正文。跨轮继续任务或需要寻找可引用证据时先调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                        "offset": {"type": "integer", "minimum": 0}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "evidence_search",
                "description": "在当前 Run 的 Evidence 正文中搜索关键词并返回脱敏片段。用于定位状态码、响应标记、漏洞特征或验证结果。",
                "parameters": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {"type": "string", "minLength": 1},
                        "evidence_id": {"type": "string", "description": "可选，仅搜索指定 Evidence"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "evidence_view",
                "description": "分页读取当前 Run 中指定 Evidence 的脱敏正文。只在摘要或搜索片段不足以支持判断时调用。",
                "parameters": {
                    "type": "object",
                    "required": ["evidence_id"],
                    "properties": {
                        "evidence_id": {"type": "string"},
                        "offset": {"type": "integer", "minimum": 0, "description": "字符偏移"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 12000}
                    }
                }
            }
        }),
    ]
}

fn finding_tool_definitions() -> Vec<Value> {
    vec![json!({
        "type": "function",
        "function": {
            "name": "record_finding",
            "description": "验证确认漏洞后立即写入当前 Run。必须引用支撑结论的 Evidence ID；重复的 title+target 会按 fingerprint 幂等忽略。仅记录已验证漏洞，疑似结果不要调用。",
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "severity", "target", "description", "confidence", "evidence_ids"],
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 500},
                    "severity": {"type": "string", "enum": ["critical", "high", "medium", "low", "info"]},
                    "target": {"type": "string", "description": "确认受影响的 URL、端点或资产"},
                    "description": {"type": "string", "description": "简明说明验证方法、实际结果和安全影响"},
                    "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                    "evidence_ids": {
                        "type": "array",
                        "minItems": 1,
                        "uniqueItems": true,
                        "items": {"type": "string", "minLength": 1}
                    }
                }
            }
        }
    })]
}

fn traffic_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "traffic_list",
                "description": "列出当前 Run 已持久化的 HTTP 流量记录（方法/URL/状态码/耗时），不返回完整报文。需要回查已发请求或选择重放目标时先调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url_contains": {"type": "string", "description": "按 URL 子串过滤"},
                        "method": {"type": "string", "description": "按 HTTP 方法过滤"},
                        "status": {"type": "integer", "description": "按响应状态码过滤"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                        "offset": {"type": "integer", "minimum": 0}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "traffic_view",
                "description": "读取当前 Run 中指定流量记录的完整请求与脱敏响应报文，支持分页。只在列表摘要不足以判断时调用。",
                "parameters": {
                    "type": "object",
                    "required": ["traffic_id"],
                    "properties": {
                        "traffic_id": {"type": "string"},
                        "offset": {"type": "integer", "minimum": 0, "description": "响应正文字符偏移"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 12000}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "traffic_sitemap",
                "description": "按 host+path 聚合当前 Run 已存储的 HTTP 流量，返回端点地图（sitemap）。显示每个路径的方法、状态码分布和请求次数，用于快速掌握已发现的目标攻击面。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "host_filter": {"type": "string", "description": "按 host 过滤，支持后缀匹配（如 example.com）"},
                        "status_min": {"type": "integer", "description": "最低响应状态码（默认 200）"},
                        "min_requests": {"type": "integer", "description": "最少请求次数阈值（默认 1）"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "traffic_diff",
                "description": "对比当前 Run 中两条流量记录的响应（状态码与正文行级差异），用于判断参数变更是否影响响应。",
                "parameters": {
                    "type": "object",
                    "required": ["traffic_id_a", "traffic_id_b"],
                    "properties": {
                        "traffic_id_a": {"type": "string"},
                        "traffic_id_b": {"type": "string"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "traffic_replay",
                "description": "按原始方法与参数重新发送当前 Run 中已存储的请求（仍受授权范围校验），返回新响应并关联原流量记录。用于复现验证或确认修复。",
                "parameters": {
                    "type": "object",
                    "required": ["traffic_id"],
                    "properties": {
                        "traffic_id": {"type": "string"}
                    }
                }
            }
        }),
    ]
}

fn skill_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "skill_list",
                "description": "列出平台已发布的 Skill 索引（名称/分类/风险/用途），不含规程正文。不确定某类验证该怎么做时先调用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "category": {"type": "string", "description": "按分类过滤，如 recon/validation"},
                        "query": {"type": "string", "description": "按名称/标题/描述关键词过滤"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "skill_load",
                "description": "按名称读取一个已发布 Skill 的完整规程（步骤/证据要求/禁止事项）。Skill 只提供方法，不扩大授权范围、不代替证据。",
                "parameters": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {"type": "string", "description": "skill_list 返回的 name"}
                    }
                }
            }
        }),
    ]
}

fn agent_tool_definitions() -> Vec<Value> {
    let agent_roles = super::agent_profiles::list()
        .map(|profiles| profiles.into_iter().map(|profile| profile.id).collect::<Vec<_>>())
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to enumerate agent profiles for tool schema");
            Vec::new()
        });
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "agent_spawn",
                "description": "派生子代理并行扫描独立目标或子任务。每个子代理独立运行工具循环，完成后结果通过 agent_collect 获取。适用于多目标并行扫描或多漏洞类型并行验证。",
                "parameters": {
                    "type": "object",
                    "required": ["description", "prompt"],
                    "properties": {
                        "description": {"type": "string", "description": "子代理简短描述，如'扫描 target1.com 的 SQL 注入'"},
                        "prompt": {"type": "string", "description": "子代理要完成的具体任务描述，包含目标和验证方法"},
                        "role": {"type": "string", "enum": agent_roles, "description":"可选 Markdown 专家角色"},
                        "target": {"type": "string", "description": "可选，子代理聚焦的单个目标 URL"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "agent_status",
                "description": "查询指定子代理的当前状态（运行中/已完成/失败），用于检查并行任务进度。",
                "parameters": {
                    "type": "object",
                    "required": ["agent_id"],
                    "properties": {
                        "agent_id": {"type": "string", "description": "agent_spawn 返回的子代理 ID"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "agent_collect",
                "description": "获取子代理的最终扫描结果。如果子代理仍在运行会阻塞等待直至完成。获取到的结果应整合进最终报告。",
                "parameters": {
                    "type": "object",
                    "required": ["agent_id"],
                    "properties": {
                        "agent_id": {"type": "string", "description": "agent_spawn 返回的子代理 ID"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "agent_list",
                "description": "列出所有已派生的子代理及其状态，用于查看并行任务全景。",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        }),
    ]
}

fn knowledge_search_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "knowledge_search",
            "description": "检索平台 Wiki 安全方法论。发现未验证线索、工具失败、缺少验证步骤或准备将线索标记为未验证时必须调用；检索后应使用当前可用工具继续验证。",
            "parameters": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "具体漏洞类型、技术栈、观察或验证目标，例如 CSP bypass DVWA、weak session ID validation"
                    }
                }
            }
        }
    })
}

fn vision_analyze_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "vision_analyze",
            "description": "用视觉模型分析截图/验证码图片，返回文字摘要。截图需以 data:image/...;base64, 数据 URL 传入；分析结果入 Evidence，不污染主模型上下文。",
            "parameters": {
                "type": "object",
                "required": ["image_data_url"],
                "properties": {
                    "image_data_url": {"type": "string", "description": "图片 data URL，如 data:image/png;base64,..."},
                    "question": {"type": "string", "description": "针对图片的具体问题，如识别验证码、登录状态、错误信息"}
                }
            }
        }
    })
}

fn file_upload_validator_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "file_upload_validate",
            "description": "在当前会话已确认的授权范围内，使用无害标记文件验证上传端点是否接受并存储文件。禁止 webshell、命令执行和持久化载荷。",
            "parameters": {
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {"type": "string", "description": "上传端点完整 URL"},
                    "field_name": {"type": "string", "description": "文件字段名，默认 imgFile"},
                    "filename": {"type": "string", "description": "标记文件名，仅允许 txt/png/svg"},
                    "content_type": {"type": "string", "description": "标记文件 MIME 类型"},
                    "verify_url": {"type": "string", "description": "可选的上传文件访问 URL"}
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        agent_tool_definitions, evidence_tool_definitions, file_upload_validator_definition,
        finding_tool_definitions, knowledge_search_definition,
    };
    use crate::conversation::tool_executor::has_conversation_handler;

    #[test]
    fn guarded_upload_schema_uses_session_authorization() {
        let definition = file_upload_validator_definition();
        let required = definition["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(required, &vec![serde_json::json!("url")]);
        assert!(
            definition["function"]["parameters"]["properties"]
                .get("confirm_authorized")
                .is_none()
        );
    }

    #[test]
    fn model_catalog_only_advertises_tools_with_real_handlers() {
        assert!(has_conversation_handler("http_request"));
        assert!(has_conversation_handler("js_analyze"));
        assert!(has_conversation_handler("knowledge_search"));
        assert!(!has_conversation_handler("tlssled"));
    }

    #[test]
    fn knowledge_search_requires_query() {
        let definition = knowledge_search_definition();
        assert_eq!(
            definition["function"]["parameters"]["required"],
            serde_json::json!(["query"])
        );
    }

    #[test]
    fn evidence_tools_are_read_only_and_run_scoped_by_runtime() {
        let definitions = evidence_tool_definitions();
        let names = definitions
            .iter()
            .filter_map(|definition| definition.pointer("/function/name"))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                &serde_json::json!("evidence_list"),
                &serde_json::json!("evidence_search"),
                &serde_json::json!("evidence_view")
            ]
        );
        assert_eq!(
            definitions[1]["function"]["parameters"]["required"],
            serde_json::json!(["query"])
        );
    }

    #[test]
    fn record_finding_requires_evidence() {
        let definitions = finding_tool_definitions();
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0]["function"]["parameters"]["required"],
            serde_json::json!([
                "title",
                "severity",
                "target",
                "description",
                "confidence",
                "evidence_ids"
            ])
        );
        assert_eq!(
            definitions[0]["function"]["parameters"]["properties"]["evidence_ids"]["minItems"],
            1
        );
    }

    #[test]
    fn agent_spawn_schema_lists_all_available_profiles() {
        let definitions = agent_tool_definitions();
        let roles = definitions[0]["function"]["parameters"]["properties"]["role"]["enum"]
            .as_array()
            .expect("agent role enum");
        assert!(roles.len() >= 12, "expected all shipped agent profiles");
        assert!(roles.contains(&serde_json::json!("recon")));
        assert!(roles.contains(&serde_json::json!("vulnerability-validator")));
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL (TEST_DATABASE_URL)"]
    async fn routed_catalog_caps_action_tools_and_keeps_reference_tools() {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        let pool = kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database");

        let definitions = super::load_routed_tool_definitions(
            &pool,
            "对 http://192.0.2.10 执行 Web 漏洞扫描与验证",
            "high",
            8,
        )
        .await
        .expect("routed catalog loads from the seeded registry");
        let names: Vec<&str> = definitions
            .iter()
            .filter_map(|definition| {
                definition
                    .pointer("/function/name")
                    .and_then(serde_json::Value::as_str)
            })
            .collect();

        const REFERENCE: &[&str] = &[
            "evidence_list",
            "evidence_search",
            "evidence_view",
            "traffic_list",
            "traffic_view",
            "traffic_diff",
            "traffic_replay",
            "knowledge_search",
            "skill_list",
            "skill_load",
        ];
        for reference in REFERENCE {
            assert!(names.contains(reference), "缺少参考工具 {reference}");
        }
        let action_tools: Vec<&&str> = names
            .iter()
            .filter(|name| !REFERENCE.contains(name) && **name != "file_upload_validate")
            .collect();
        assert!(
            action_tools.len() <= 8,
            "动作工具必须被路由截断到 top_k，实际 {} 个：{action_tools:?}",
            action_tools.len()
        );
        assert!(
            !action_tools.is_empty(),
            "种子注册表应至少路由出一个动作工具"
        );
        for name in &action_tools {
            assert!(
                super::has_conversation_handler(name),
                "路由结果包含无对话处理器的工具：{name}"
            );
        }
        for definition in &definitions {
            assert!(definition.pointer("/function/description").is_some());
            assert!(definition.pointer("/function/parameters").is_some());
        }
    }
}

// ── Routed tool catalog (top-K schema injection) ─────────────────────────

use kairos_platform_pent_api::{
    ToolDescriptor, ToolExecutorSpec, ToolHealthStatus, ToolLifecycleStatus, ToolQualityMetrics,
    ToolResourceProfile, ToolRouteRequest, ToolSource,
};

#[derive(sqlx::FromRow)]
struct DescriptorRow {
    tool_id: String,
    version: String,
    manifest_version: i32,
    namespace: String,
    name: String,
    description: String,
    capabilities: Value,
    tags: Value,
    source: String,
    input_schema: Value,
    output_schema: Value,
    risk_level: String,
    approval_policy: String,
    supported_target_types: Value,
    executor: Value,
    resource_profile: Value,
    embedding_document: String,
    revision: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// Default number of action-tool schemas injected per model call. Reference
/// (evidence/traffic/knowledge) tools are always appended on top.
const DEFAULT_TOOL_SCHEMA_TOP_K: usize = 0; // 0 = no truncation, give all tools to the model

pub fn tool_schema_top_k() -> usize {
    std::env::var("PENTA_TOOL_SCHEMA_TOP_K")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_TOOL_SCHEMA_TOP_K)
}

/// Load action-tool schemas via policy-aware routing (hard filter +
/// deterministic scoring, `pent-core::tool_router`) instead of dumping every
/// active registry tool into the model context.
///
/// ## Retrieval pipeline
///
/// 1. FTS+vector hybrid retrieval narrows the candidate set (semantic match
///    against the routing intent)
/// 2. Hard filter (lifecycle / health / risk / namespace / capabilities)
/// 3. Deterministic scoring (capability match / keyword / quality / health)
/// 4. Top-k truncation
///
/// Each step degrades deterministically: retrieval failure → full catalog;
/// filter failure → reject; scoring always produces a ranking.
///
/// Reference (evidence/traffic/knowledge/skill) tools are always appended
/// regardless of the routing decision.
pub async fn load_routed_tool_definitions(
    pool: &PgPool,
    intent: &str,
    max_risk: &str,
    top_k: usize,
) -> anyhow::Result<Vec<Value>> {
    // ── Step 1: Hybrid retrieval ──────────────────────────────────────
    // Try FTS+vector retrieval to narrow the candidate set. On failure or
    // empty results, degrade to loading the full active catalog.
    let retrieved = retrieve_tool_ids(pool, intent, top_k * 5).await;
    let rows: Vec<DescriptorRow> = if !retrieved.is_empty() {
        let ids: Vec<String> = retrieved.iter().map(|(id, _)| id.clone()).collect();
        tracing::debug!(
            count = ids.len(),
            "hybrid retrieval narrowed tool candidate set"
        );
        sqlx::query_as::<_, DescriptorRow>(
            "SELECT tool_id, version, manifest_version, namespace, name, description,
                    capabilities, tags, source, input_schema, output_schema,
                    risk_level, approval_policy, supported_target_types,
                    executor, resource_profile, embedding_document, revision,
                    created_at, updated_at
             FROM pent_tool_definitions
             WHERE lifecycle_status = 'active'
               AND tool_id = ANY($1)
             ORDER BY namespace, name",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await?
    } else {
        // Fallback: load all active tools (deterministic degradation)
        tracing::debug!("retrieval returned no results; loading full catalog");
        sqlx::query_as::<_, DescriptorRow>(
            "SELECT tool_id, version, manifest_version, namespace, name, description,
                    capabilities, tags, source, input_schema, output_schema,
                    risk_level, approval_policy, supported_target_types,
                    executor, resource_profile, embedding_document, revision,
                    created_at, updated_at
             FROM pent_tool_definitions
             WHERE lifecycle_status = 'active'
             ORDER BY namespace, name",
        )
        .fetch_all(pool)
        .await?
    };

    let mut descriptors = Vec::with_capacity(rows.len());
    for row in rows {
        let descriptor = descriptor_from_row(row)?;
        if has_conversation_handler(&descriptor.tool_id) {
            descriptors.push(descriptor);
        }
    }
    anyhow::ensure!(
        !descriptors.is_empty(),
        "注册表中没有可在对话模式执行的工具"
    );

    let request = ToolRouteRequest {
        run_id: String::new(),
        tenant_id: 0,
        project_id: None,
        phase: "conversation".into(),
        intent: intent.chars().take(800).collect(),
        target: String::new(),
        target_type: "url".into(),
        required_capabilities: vec![],
        allowed_namespaces: vec![],
        completed_tools: vec![],
        max_risk: max_risk.to_string(),
        budget_remaining: json!({}),
        context: json!({}),
        top_k,
    };
    let revision = descriptors
        .iter()
        .map(|tool| tool.revision)
        .max()
        .unwrap_or(0);
    let decision =
        kairos_platform_pent_core::tool_router::route_tools(&request, &descriptors, revision, 1);
    anyhow::ensure!(
        !decision.selected.is_empty(),
        "工具路由未选出任何候选（全部硬过滤）"
    );
    tracing::info!(
        selected = ?decision.selected.iter().map(|c| c.tool_id.as_str()).collect::<Vec<_>>(),
        rejected = decision.rejected_count,
        retrieval_candidates = descriptors.len(),
        hybrid_retrieval = !retrieved.is_empty(),
        "conversation tool routing decision"
    );

    let mut definitions = Vec::new();
    for candidate in &decision.selected {
        let Some(descriptor) = descriptors
            .iter()
            .find(|tool| tool.tool_id == candidate.tool_id)
        else {
            continue;
        };
        definitions.push(json!({
            "type": "function",
            "function": {
                "name": descriptor.tool_id,
                "description": descriptor.description,
                "parameters": descriptor.input_schema
            }
        }));
    }

    // Reference (fact-layer) tools are always available so the model can read
    // evidence/traffic/knowledge regardless of the current routing intent.
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("file_upload_validate")
    }) {
        definitions.push(file_upload_validator_definition());
    }
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("knowledge_search")
    }) {
        definitions.push(knowledge_search_definition());
    }
    if !definitions.iter().any(|definition| {
        definition.pointer("/function/name").and_then(Value::as_str) == Some("vision_analyze")
    }) {
        definitions.push(vision_analyze_definition());
    }
    for definition in evidence_tool_definitions()
        .into_iter()
        .chain(traffic_tool_definitions())
        .chain(finding_tool_definitions())
        .chain(skill_tool_definitions())
        .chain(agent_tool_definitions())
    {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions
            .iter()
            .any(|existing| existing.pointer("/function/name").and_then(Value::as_str) == name)
        {
            definitions.push(definition);
        }
    }
    Ok(definitions)
}

fn descriptor_from_row(row: DescriptorRow) -> anyhow::Result<ToolDescriptor> {
    let DescriptorRow {
        tool_id,
        version,
        manifest_version,
        namespace,
        name,
        description,
        capabilities,
        tags,
        source,
        input_schema,
        output_schema,
        risk_level,
        approval_policy,
        supported_target_types,
        executor,
        resource_profile,
        embedding_document,
        revision,
        created_at,
        updated_at,
    } = row;
    Ok(ToolDescriptor {
        tool_id,
        version,
        manifest_version,
        namespace,
        name,
        description,
        capabilities: string_list(&capabilities),
        tags: string_list(&tags),
        source: descriptor_source(&source, &executor)?,
        input_schema,
        output_schema,
        risk_level,
        approval_policy,
        supported_target_types: string_list(&supported_target_types),
        executor: ToolExecutorSpec {
            kind: descriptor_source(&source, &executor)?,
            endpoint: executor
                .get("endpoint")
                .and_then(Value::as_str)
                .map(str::to_owned),
            server_id: executor
                .get("server_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            handler: executor
                .get("handler")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        resource_profile: ToolResourceProfile {
            cpu_cores: resource_profile
                .get("cpu_cores")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32,
            memory_mb: resource_profile
                .get("memory_mb")
                .and_then(Value::as_u64)
                .unwrap_or(256),
            timeout_seconds: resource_profile
                .get("timeout_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(300),
            max_concurrency: resource_profile
                .get("max_concurrency")
                .and_then(Value::as_u64)
                .unwrap_or(1) as u32,
            network_policy: resource_profile
                .get("network_policy")
                .and_then(Value::as_str)
                .unwrap_or("scope_only")
                .to_string(),
        },
        // Health is probed on demand by the management API; active registry
        // tools are assumed routable until marked otherwise.
        health_status: ToolHealthStatus::Ready,
        lifecycle_status: ToolLifecycleStatus::Active,
        embedding_document,
        embedding_model: None,
        embedding_version: None,
        vector_record_id: None,
        quality: ToolQualityMetrics::default(),
        revision,
        created_at,
        updated_at,
    })
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_tool_source(value: &str) -> anyhow::Result<ToolSource> {
    match value {
        "builtin" => Ok(ToolSource::Builtin),
        // The host backend is a local-process executor (test-only); the
        // descriptor enum has no Host variant, so it maps to Builtin.
        "host" => Ok(ToolSource::Builtin),
        "container" => Ok(ToolSource::Builtin),
        "mcp" => Ok(ToolSource::Mcp),
        "http" => Ok(ToolSource::Http),
        other => anyhow::bail!("未知工具执行类型: {other}"),
    }
}

/// `source` in the registry is provenance (`builtin` shipped vs `external`
/// registered), not the execution backend; the backend lives in
/// `executor.type`. Externally-registered tools take their source from the
/// executor so routing and health semantics stay accurate.
fn descriptor_source(source: &str, executor: &Value) -> anyhow::Result<ToolSource> {
    let executor_kind = executor
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(source);
    if source == "builtin" {
        return Ok(ToolSource::Builtin);
    }
    parse_tool_source(executor_kind)
}
