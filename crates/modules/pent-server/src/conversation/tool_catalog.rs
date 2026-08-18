use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_api::ToolExecutionClass;
use kairos_platform_pent_tooling::catalog::ToolDefinition;
use kairos_platform_pent_tooling::manifest::{ToolManifestRecord, descriptor_execution_class};
use serde_json::{Value, json};

use super::tool_executor::has_conversation_handler;
use super::tool_retrieval::retrieve_tool_ids;

fn tool_definition(schema: Value, execution_class: ToolExecutionClass) -> ToolDefinition {
    ToolDefinition::new(schema, execution_class)
}

fn inline_definition(schema: Value) -> ToolDefinition {
    tool_definition(schema, ToolExecutionClass::Inline)
}

/// Load tool schemas from the maintained registry instead of duplicating them
/// in the conversation runtime.
pub async fn load_conversation_tool_definitions(
    pool: &PgPool,
) -> anyhow::Result<Vec<ToolDefinition>> {
    let rows = sqlx::query_as::<_, (String, String, Value, String, String, Value)>(
        "SELECT tool_id, description, input_schema, risk_level, approval_policy, executor
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

    let unavailable = match super::tool_health::recently_unavailable_tools(pool).await {
        Ok(unavailable) => unavailable,
        Err(error) => {
            // Health is an availability hint, not a reason to make the entire
            // model catalog unavailable during a control-plane degradation.
            tracing::warn!(%error, "tool health lookup failed; keeping active catalog");
            std::collections::HashSet::new()
        }
    };

    let mut definitions = rows
        .into_iter()
        .filter(|(name, _, _, _, _, _)| !unavailable.contains(&name.to_ascii_lowercase()))
        .filter(|(name, _, _, _, _, _)| has_conversation_handler(name))
        .map(
            |(name, description, parameters, risk_level, approval_policy, executor)| {
                let execution_class =
                    descriptor_execution_class(&risk_level, &approval_policy, &executor);
                let schema = json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description,
                        "parameters": parameters
                    }
                });
                tool_definition(super::tool_guidance::enhance_tool_schema(schema), execution_class)
            },
        )
        .collect::<Vec<_>>();

    // DesRed-style MCP-first extension point.  Server qualification keeps two
    // providers with the same tool name unambiguous for the model and routes
    // the call back to the selected MCP server without a second lookup.
    let registry = kairos_platform_pent_tooling::mcp::registry::shared().await;
    let mcp_tools = registry.all_tools().await;
    for (server_id, tool) in mcp_tools {
        definitions.push(tool_definition(
            json!({
                "type": "function",
                "function": {
                    "name": format!("mcp_{server_id}__{}", tool.name),
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            }),
            ToolExecutionClass::Destructive,
        ));
    }

    // This guarded validator is implemented by the conversation runtime but is
    // not yet part of older database snapshots.
    if !definitions
        .iter()
        .any(|definition| definition.name() == Some("file_upload_validate"))
    {
        definitions.push(inline_definition(file_upload_validator_definition()));
    }
    if !definitions
        .iter()
        .any(|definition| definition.name() == Some("knowledge_search"))
    {
        definitions.push(inline_definition(knowledge_search_definition()));
    }
    if !definitions
        .iter()
        .any(|definition| definition.name() == Some("vision_analyze"))
    {
        definitions.push(inline_definition(vision_analyze_definition()));
    }
    for definition in evidence_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
        }
    }
    for definition in traffic_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
        }
    }
    for definition in finding_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
        }
    }
    for definition in skill_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
        }
    }
    for definition in super::async_tools::definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
        }
    }
    if !definitions
        .iter()
        .any(|definition| definition.name() == Some("tool_search"))
    {
        definitions.push(inline_definition(tool_search_definition()));
    }
    for definition in agent_tool_definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
        }
    }
    for definition in super::plan_board::definitions() {
        let name = definition.pointer("/function/name").and_then(Value::as_str);
        if !definitions.iter().any(|existing| existing.name() == name) {
            definitions.push(inline_definition(definition));
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
    unlocked: &std::collections::HashSet<String>,
) -> anyhow::Result<Vec<ToolDefinition>> {
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
        .unwrap_or(12)
        .clamp(6, 16);
    let retrieved: std::collections::HashSet<String> = retrieve_tool_ids(pool, intent, top_k)
        .await
        .into_iter()
        .map(|(id, _)| id.to_ascii_lowercase())
        .collect();
    let mut explicitly_unlocked = explicit_tool_names(intent);
    explicitly_unlocked.extend(unlocked.iter().map(|name| name.to_ascii_lowercase()));
    const ALWAYS: &[&str] = &[
        "knowledge_search",
        "evidence_list",
        "evidence_search",
        "evidence_view",
        "query_execution_result",
        "traffic_list",
        "traffic_view",
        "traffic_diff",
        "traffic_replay",
        "skill_list",
        "skill_load",
        "tool_search",
        "start_tool_execution",
        "get_tool_execution",
        "wait_tool_execution",
        "cancel_tool_execution",
        "agent_spawn",
        "agent_status",
        "agent_collect",
        "agent_list",
        "record_finding",
        "plan_create",
        "plan_get",
        "plan_update",
        "read_file",
        "glob",
        "grep",
    ];
    // A retrieval miss must stay bounded. These generic discovery tools let
    // the model establish enough context for a more specific `tool_search`
    // without dumping the complete registry into the prompt.
    const FALLBACK: &[&str] = &[
        "http_probe",
        "http_request",
        "dns_lookup",
        "tcp_scan",
        "nmap",
        "web_crawl",
        "web_fingerprint",
        "nuclei",
    ];
    let retrieval_missed = retrieved.is_empty();
    let selected = all
        .into_iter()
        .filter(|definition| {
            let name = definition.name().unwrap_or("");
            let lower = name.to_ascii_lowercase();
            ALWAYS.contains(&lower.as_str())
                || (retrieval_missed && FALLBACK.contains(&lower.as_str()))
                || retrieved.contains(&lower)
                || explicitly_unlocked.contains(&lower)
        })
        .collect::<Vec<_>>();
    tracing::debug!(
        available = selected.len(),
        retrieved = retrieved.len(),
        "dynamic tool disclosure applied"
    );
    Ok(selected)
}

fn explicit_tool_names(intent: &str) -> std::collections::HashSet<String> {
    let Ok(pattern) = regex::Regex::new(r#"\\?"name\\?"\s*:\s*\\?"([A-Za-z0-9_.:-]+)\\?""#) else {
        return std::collections::HashSet::new();
    };
    pattern
        .captures_iter(intent)
        .filter_map(|captures| captures.get(1))
        .map(|value| value.as_str().to_ascii_lowercase())
        .collect()
}

pub(super) fn tool_search_result_names(content: &str) -> std::collections::HashSet<String> {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return std::collections::HashSet::new();
    };
    value
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .map(|name| name.to_ascii_lowercase())
        .collect()
}

/// Search the complete active tool registry on demand. This is intentionally
/// callable from the model loop so a narrowly disclosed catalog can expand
/// after discovery reveals a new protocol, endpoint shape, or vulnerability.
pub async fn search_tool_catalog(
    pool: &PgPool,
    query: &str,
    limit: usize,
) -> anyhow::Result<Value> {
    let query = query.trim();
    anyhow::ensure!(!query.is_empty(), "tool_search 缺少 query");
    let limit = limit.clamp(1, 30);
    let definitions = load_conversation_tool_definitions(pool).await?;
    let ranked = retrieve_tool_ids(pool, query, limit * 3).await;
    let ranks = ranked
        .into_iter()
        .enumerate()
        .map(|(index, (name, _))| (name.to_ascii_lowercase(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let terms = query
        .to_ascii_lowercase()
        .split(|character: char| {
            !character.is_alphanumeric() && character != '_' && character != '-'
        })
        .filter(|term| !term.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut candidates = definitions
        .into_iter()
        .filter(|definition| definition.name() != Some("tool_search"))
        .filter_map(|definition| {
            let name = definition.name()?.to_string();
            let lower_name = name.to_ascii_lowercase();
            let document = format!(
                "{} {}",
                lower_name,
                definition
                    .schema
                    .pointer("/function/description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            );
            let lexical_score = terms
                .iter()
                .filter(|term| document.contains(term.as_str()))
                .count();
            let rank = ranks.get(&lower_name).copied();
            (rank.is_some() || lexical_score > 0).then_some((
                rank,
                lexical_score,
                name,
                definition.schema,
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.0
            .unwrap_or(usize::MAX)
            .cmp(&right.0.unwrap_or(usize::MAX))
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    candidates.truncate(limit);
    let tools = candidates
        .into_iter()
        .map(|(_, _, name, schema)| {
            json!({
                "name": name,
                "description": schema.pointer("/function/description").cloned().unwrap_or(Value::Null),
                "parameters": schema.pointer("/function/parameters").cloned().unwrap_or_else(|| json!({"type":"object"}))
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "query": query,
        "count": tools.len(),
        "tools": tools,
        "instruction": "下一轮直接按返回的精确 name 调用所需工具；若无结果，改用更具体的协议、漏洞类别或工具名再次搜索。"
    }))
}

fn evidence_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "query_execution_result",
                "description": "按 execution_id 分页读取当前 Run 内已持久化的工具结果，可按行搜索和过滤。不会访问其他 Run。",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["execution_id"],
                    "properties": {
                        "execution_id": {"type": "string", "minLength": 1, "maxLength": 100},
                        "page": {"type": "integer", "minimum": 1},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 500},
                        "search": {"type": "string", "maxLength": 500},
                        "filter": {"type": "string", "maxLength": 500}
                    }
                }
            }
        }),
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
                "description": "按相关度列出平台已发布的 Skill 索引（名称/分类/风险/用途），不含规程正文。必须提供 query 或 category；任务阶段或漏洞类型变化时重新检索。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "category": {"type": "string", "description": "按分类过滤，如 recon/validation"},
                        "query": {"type": "string", "description": "按名称/标题/描述关键词过滤"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 8}
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

fn tool_search_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "tool_search",
            "description": "按当前新发现的协议、技术栈、漏洞类型或工具名动态检索完整工具注册表。当前可见工具不足时调用，返回下一轮可直接调用的精确工具名和参数结构。",
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "required": ["query"],
                "properties": {
                    "query": {"type": "string", "minLength": 1, "description": "具体能力需求，例如 GraphQL introspection、SSRF、JWT、SMB enumeration"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 30, "default": 12}
                }
            }
        }
    })
}

fn agent_tool_definitions() -> Vec<Value> {
    let agent_roles = super::agent_profiles::list()
        .map(|profiles| {
            profiles
                .into_iter()
                .map(|profile| profile.id)
                .collect::<Vec<_>>()
        })
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
                    "verify_url": {"type": "string", "description": "可选的上传文件访问 URL"},
                    "form": {"type": "object", "description": "上传表单附加字段；user_token 会从当前 Run 自动注入", "additionalProperties": {"type": "string"}},
                    "cookies": {"type": "string", "description": "认证 Cookie；通常由当前 Run 自动注入"}
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        agent_tool_definitions, descriptor_execution_class, evidence_tool_definitions,
        explicit_tool_names, file_upload_validator_definition, finding_tool_definitions,
        knowledge_search_definition, load_tools_for_intent, tool_search_definition,
        tool_search_result_names,
    };
    use crate::conversation::tool_guidance::enhance_tool_schema;
    use crate::conversation::tool_executor::has_conversation_handler;
    use kairos_platform_pent_runtime::ports::ToolExecutionClass;

    #[test]
    fn descriptor_execution_metadata_overrides_risk_fallback() {
        assert_eq!(
            descriptor_execution_class(
                "critical",
                "always",
                &serde_json::json!({"execution_class": "read_only"}),
            ),
            ToolExecutionClass::ReadOnly
        );
        assert_eq!(
            descriptor_execution_class("high", "conditional", &serde_json::json!({})),
            ToolExecutionClass::Destructive
        );
        assert_eq!(
            descriptor_execution_class(
                "low",
                "conditional",
                &serde_json::json!({"execution_class": "invalid"}),
            ),
            ToolExecutionClass::Destructive
        );
    }

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
    fn tool_search_schema_and_results_unlock_exact_tool_names() {
        let definition = tool_search_definition();
        assert_eq!(
            definition["function"]["parameters"]["required"],
            serde_json::json!(["query"])
        );
        let names =
            explicit_tool_names(r#"{"tools":[{"name":"sqlmap_scan"},{"name":"web_xss_scan"}]}"#);
        assert!(names.contains("sqlmap_scan"));
        assert!(names.contains("web_xss_scan"));
        let names = tool_search_result_names(
            r#"{"tools":[{"name":"sqlmap_scan"},{"name":"web_xss_scan"}]}"#,
        );
        assert!(names.contains("sqlmap_scan"));
    }

    #[tokio::test]
    async fn retrieval_miss_never_discloses_the_full_catalog() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
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
        let all = super::load_conversation_tool_definitions(&pool)
            .await
            .expect("load complete catalog");
        let selected = load_tools_for_intent(
            &pool,
            "完全不存在的检索词甲乙丙丁",
            &serde_json::json!({
                "tool_search_enabled": true,
                "tool_search_min_tools": 20,
                "tool_search_top_k": 12
            }),
            &std::collections::HashSet::new(),
        )
        .await
        .expect("load bounded catalog");
        assert!(selected.len() < all.len());
        assert!(selected.len() <= 40, "候选仍过多：{}", selected.len());
        assert!(
            selected
                .iter()
                .any(|tool| tool.name() == Some("tool_search"))
        );
    }

    #[test]
    fn critical_tool_descriptions_explain_usage_output_and_boundary() {
        let definition = enhance_tool_schema(serde_json::json!({
            "type": "function",
            "function": {
                "name": "hakrawler",
                "description": "快速 Web 爬虫。",
                "parameters": {"type": "object"}
            }
        }));
        let description = definition["function"]["description"].as_str().unwrap();
        assert!(description.contains("端点地图"));
        assert!(description.contains("arjun/x8"));
        assert!(description.contains("不等于漏洞验证"));
    }

    #[test]
    fn execution_result_query_is_run_scoped_and_bounded() {
        let definition = evidence_tool_definitions()
            .into_iter()
            .find(|value| value["function"]["name"] == "query_execution_result")
            .expect("query_execution_result definition");
        assert_eq!(
            definition["function"]["parameters"]["required"],
            serde_json::json!(["execution_id"])
        );
        assert_eq!(
            definition["function"]["parameters"]["properties"]["limit"]["maximum"],
            500
        );
        assert!(
            definition["function"]["description"]
                .as_str()
                .unwrap()
                .contains("当前 Run")
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
                &serde_json::json!("query_execution_result"),
                &serde_json::json!("evidence_list"),
                &serde_json::json!("evidence_search"),
                &serde_json::json!("evidence_view")
            ]
        );
        assert_eq!(
            definitions[2]["function"]["parameters"]["required"],
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
    async fn routed_catalog_caps_action_tools_and_keeps_reference_tools() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
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
            "tool_search",
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

use kairos_platform_pent_api::{ToolDescriptor, ToolRouteRequest};

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
        definitions.push(super::tool_guidance::enhance_tool_schema(json!({
            "type": "function",
            "function": {
                "name": descriptor.tool_id,
                "description": descriptor.description,
                "parameters": descriptor.input_schema
            }
        })));
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
        .chain(std::iter::once(tool_search_definition()))
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
    ToolManifestRecord {
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
    }
    .into_descriptor()
}
