//! Tool executor for the conversation agent.
//! Provides real implementations of basic tools that can be used
//! during conversation-driven penetration testing.
use std::time::Instant;

pub use kairos_platform_pent_runtime::ports::RuntimeToolResult as ConversationToolResult;
use kairos_platform_pent_runtime::ports::{
    ConversationToolExecutor, RuntimeToolCall, ToolExecutionContext,
};

const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 50_000;
const NETWORK_TOOLS: &[&str] = &[
    "dir_fuzz",
    "dns_lookup",
    "file_upload_validate",
    "http_probe",
    "http_request",
    "js_collect",
    "web_crawl",
    "web_fingerprint",
    "web_url_collect",
];

fn direct_network_tools_enabled() -> bool {
    cfg!(test)
        || (std::env::var("PENTA_ENV").as_deref() == Ok("development")
            && std::env::var("PENTA_ALLOW_DIRECT_NETWORK_TOOLS").as_deref() == Ok("true"))
}

fn conversation_native_tool(tool_name: &str) -> bool {
    matches!(tool_name, "http_probe" | "http_request")
}

/// Whether this process has a real conversation-mode handler for a registry
/// tool. The database remains the source of truth for which tools are active;
/// this predicate prevents the model from being offered catalog entries whose
/// worker executors are not implemented yet. Any recipe-backed tool is
/// executable (via `prepare_tool` / `execute_tool_json`), so it qualifies.
pub(super) fn has_conversation_handler(tool_name: &str) -> bool {
    let implemented = matches!(
        tool_name,
        // ── Conversation-native tools ────────────────────────────────────
        "dir_fuzz"
            | "dirsearch"
            | "dns_lookup"
            | "file_upload_validate"
            | "http_probe"
            | "http_request"
            | "js_analyze"
            | "js_collect"
            | "knowledge_search"
            | "vision_analyze"
            | "evidence_list"
            | "evidence_search"
            | "evidence_view"
            | "query_execution_result"
            | "query-execution-result"
            | "record_finding"
            | "traffic_list"
            | "traffic_view"
            | "traffic_diff"
            | "traffic_replay"
            | "skill_list"
            | "skill_load"
            | "web_crawl"
            | "web_fingerprint"
            | "web_url_collect"
            // ── Run-mode container tools (via tool_registry) ────────────
            | "tcp_scan"
            | "nmap"
            | "nmap_scan"
            | "nuclei"
            | "web_vuln_scan"
            | "sqli_validate"
            | "xss_validate"
            | "csrf_validate"
            | "command_injection_validate"
            | "file_inclusion_validate"
            | "afrog"
            | "ssh_exec"
            | "sql_query"
            | "sqlmap"
            | "sqlmap_scan"
            | "subfinder"
            | "amass"
            | "dnsx"
            | "masscan"
            | "naabu"
            | "ffuf"
            | "gobuster"
            | "nikto"
            | "whatweb"
            | "wafw00f"
            | "httpx"
            | "wpscan"
            | "web_wordpress_scan"
            | "xsser"
            | "web_xss_scan"
            | "dalfox"
            | "commix"
            | "testssl"
            | "web_tls_scan"
            | "feroxbuster"
            | "wfuzz"
            | "hakrawler"
            | "katana"
            | "gau"
            | "x8"
            | "httrack"
            | "arjun"
            | "dirb"
            | "dnsrecon"
            | "dnsenum"
            | "fierce"
            | "dmitry"
            | "theharvester"
            | "enum4linux"
            | "smbmap"
            | "smtp-user-enum"
            | "swaks"
            | "davtest"
            | "nbtscan"
            | "netdiscover"
            | "sslyze"
            | "qsslcaudit"
            | "wapiti"
            | "skipfish"
            | "lbd"
            | "whois"
            | "whois_lookup"
            | "sslscan"
            | "tls_inspect"
            | "curl"
            // ── Builtin local tools ────────────────────────────────────────
            | "read"
            | "grep"
            | "web_search"
    );
    let implemented = implemented
        || tool_name.starts_with("mcp_")
        || crate::infrastructure::recipe::has_executable(tool_name);
    implemented
        && (!NETWORK_TOOLS.contains(&tool_name)
            || conversation_native_tool(tool_name)
            || direct_network_tools_enabled())
}

/// Server adapter that binds the runtime ToolExecutor port to the existing
/// conversation tool router (builtin, recipe, MCP and HTTP handlers).
pub struct ServerConversationToolExecutor;

#[async_trait::async_trait]
impl ConversationToolExecutor for ServerConversationToolExecutor {
    async fn execute(
        &self,
        call: &RuntimeToolCall,
        context: &ToolExecutionContext,
    ) -> ConversationToolResult {
        let mut result = execute_tool(
            &call.name,
            &call.arguments,
            &context.scope_snapshot,
            &context.session_id,
            context.run_id.as_deref(),
            Some(&call.id),
        )
        .await;
        result.tool_call_id.clone_from(&call.id);
        result
    }
}

/// Execute a tool call from the LLM. Returns the result that can be
/// fed back to the model in the next turn.
pub async fn execute_tool(
    name: &str,
    arguments: &str,
    scope_snapshot: &serde_json::Value,
    session_id: &str,
    run_id: Option<&str>,
    execution_id: Option<&str>,
) -> ConversationToolResult {
    if NETWORK_TOOLS.contains(&name)
        && !conversation_native_tool(name)
        && !direct_network_tools_enabled()
    {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: name.into(),
            content: serde_json::json!({
                "error": "对话进程内网络工具默认关闭；请通过隔离 Worker 的 Run 执行",
                "blocked": true
            })
            .to_string(),
            success: false,
        };
    }
    if let Err(error) = super::tool_guard::validate_tool_scope(name, arguments, scope_snapshot) {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: name.into(),
            content: serde_json::json!({"error": error, "blocked": true}).to_string(),
            success: false,
        };
    }
    match name {
        // ── HTTP tools ──────────────────────────────────────────
        "http_request" if !direct_network_tools_enabled() => {
            execute_isolated_http_request(
                arguments,
                scope_snapshot,
                session_id,
                run_id,
                execution_id,
            )
            .await
        }
        "http_probe" if !direct_network_tools_enabled() => {
            execute_isolated_http_probe(
                arguments,
                scope_snapshot,
                session_id,
                run_id,
                execution_id,
            )
            .await
        }
        "http_request" => execute_http_request(arguments).await,
        "http_probe" => execute_http_probe(arguments).await,

        // ── DNS tools ───────────────────────────────────────────
        "dns_lookup" => execute_dns_lookup(arguments).await,

        // ── Web tools ───────────────────────────────────────────
        "web_crawl" | "web_fingerprint" | "web_url_collect" => execute_web_crawl(arguments).await,
        "js_collect" => super::tool_extensions::execute_js_collect(arguments).await,
        "js_analyze" => super::tool_extensions::execute_js_analyze(arguments),
        "dir_fuzz" => super::tool_extensions::execute_dir_fuzz(arguments).await,
        "file_upload_validate" => {
            super::tool_extensions::execute_file_upload_validate(arguments).await
        }

        // ── Builtin local tools ────────────────────────────────────
        "read" => execute_builtin_local("read", arguments).await,
        "grep" => execute_builtin_local("grep", arguments).await,
        "web_search" => execute_builtin_local("web_search", arguments).await,

        // ── MCP tools (dynamic) ────────────────────────────────────
        name if name.starts_with("mcp_") => execute_mcp_tool(name, arguments).await,

        // ── Fallback ────────────────────────────────────────────
        other => {
            // Recipe-backed host tools with a `parameters` mapping render from
            // the model's structured JSON arguments (DesRed-style) and run
            // through the sandboxed host executor with scope-derived policy.
            if let Some(recipe) = crate::infrastructure::recipe::lookup(other) {
                if !recipe.parameters.is_empty() {
                    return execute_recipe_tool(
                        other,
                        arguments,
                        scope_snapshot,
                        session_id,
                        run_id,
                        execution_id,
                    )
                    .await;
                }
            }
            ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: other.into(),
                content: serde_json::json!({
                    "error": format!("工具 '{}' 在对话模式中不可用，请通过 Run 方式执行", other),
                    "suggestion": "如需使用此工具，请创建渗透任务并在 Run 中执行"
                })
                .to_string(),
                success: false,
            }
        }
    }
}

/// Execute a recipe-backed host tool from structured JSON arguments.
async fn execute_recipe_tool(
    name: &str,
    arguments: &str,
    scope_snapshot: &serde_json::Value,
    session_id: &str,
    run_id: Option<&str>,
    execution_id: Option<&str>,
) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(value) => value,
        Err(error) => return tool_error(name, format!("参数解析失败: {error}")),
    };
    let Some(bus) = crate::transport::mq::conversation_tool_bus() else {
        return tool_error(
            name,
            "pent-agent 工具执行通道不可用；请检查 NATS 和 pent-agent 状态",
        );
    };
    let request = crate::transport::mq::RemoteToolExecutionRequest {
        execution_id: execution_id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        tool_name: name.to_string(),
        arguments: args,
        scope_snapshot: scope_snapshot.clone(),
        session_id: session_id.to_string(),
        run_id: run_id.map(ToOwned::to_owned),
        traffic_run_id: None,
        timeout_seconds: crate::infrastructure::recipe::lookup(name)
            .map(|recipe| recipe.resource_profile.timeout_seconds)
            .unwrap_or(300),
    };
    match bus.request_tool_execution(&request).await {
        Ok(response) if response.error.is_some() => {
            tool_error(name, response.error.unwrap_or_default())
        }
        Ok(response) if response.output.is_none() => {
            tool_error(name, "pent-agent 返回了空执行结果")
        }
        Ok(response) => {
            let output = response.output.expect("checked above");
            ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: name.into(),
                content: serde_json::json!({
                    "stdout": output.stdout,
                    "stderr": output.stderr,
                    "exit_code": output.exit_code,
                    "duration_ms": output.duration_ms,
                    "execution_audit": output.audit,
                })
                .to_string(),
                success: output.exit_code == 0,
            }
        }
        Err(error) => tool_error(name, error),
    }
}

// ── Builtin local tool dispatch ────────────────────────────────────────

async fn execute_builtin_local(name: &str, arguments: &str) -> ConversationToolResult {
    use crate::tooling::builtin::local_tools;
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return tool_error(name, format!("参数解析失败: {e}")),
    };
    let result = match name {
        "read" => local_tools::execute_read(args).await,
        "grep" => local_tools::execute_grep(args).await,
        "web_search" => local_tools::execute_web_search(args).await,
        _ => {
            return tool_error(name, format!("本地工具 '{name}' 尚未实现"));
        }
    };
    match result {
        Ok(value) => ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: name.into(),
            content: value.to_string(),
            success: true,
        },
        Err(error) => tool_error(name, format!("{error}")),
    }
}

// ── MCP tool dispatch ──────────────────────────────────────────────────

fn parse_mcp_tool_name(name: &str) -> (Option<&str>, &str) {
    let qualified = name.strip_prefix("mcp_").unwrap_or(name);
    match qualified.split_once("__") {
        Some((server_id, tool_name)) if !server_id.is_empty() && !tool_name.is_empty() => {
            (Some(server_id), tool_name)
        }
        _ => (None, qualified),
    }
}

async fn execute_mcp_tool(name: &str, arguments: &str) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => return tool_error(name, format!("参数解析失败: {e}")),
    };

    // Tool disclosure, capability-center lifecycle operations and execution
    // must all observe the same process-wide registry. Database-configured
    // servers are attached to this registry by pent-agent at startup.
    let registry = kairos_platform_pent_tooling::mcp::registry::shared().await;
    if registry.server_count().await == 0 {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: name.into(),
            content: serde_json::json!({
                "error": "MCP 服务未配置或未启动",
                "hint": "检查能力中心 MCP 配置或 PENTA_MCP_SERVERS"
            })
            .to_string(),
            success: false,
        };
    }

    // Catalog names are server-qualified (`mcp_{server_id}__{tool_name}`) so
    // tools with the same name on different providers remain unambiguous.
    let all_tools = registry.all_tools().await;
    let (qualified_server, mcp_tool_name) = parse_mcp_tool_name(name);
    let server_id = match all_tools.iter().find(|(server_id, tool)| {
        tool.name == mcp_tool_name
            && qualified_server.is_none_or(|requested| requested == server_id)
    }) {
        Some((sid, _)) => sid.clone(),
        None => {
            return tool_error(
                name,
                format!("MCP 工具 '{mcp_tool_name}' 在已连接的服务器中未找到"),
            );
        }
    };

    match registry.call_tool(&server_id, mcp_tool_name, args).await {
        Ok(result) => {
            let text: String = result
                .content
                .iter()
                .filter_map(|block| match block {
                    kairos_platform_pent_tooling::mcp::ContentBlock::Text { text } => {
                        Some(text.clone())
                    }
                    kairos_platform_pent_tooling::mcp::ContentBlock::Image {
                        mime_type, ..
                    } => Some(format!("[image: {mime_type}]")),
                    kairos_platform_pent_tooling::mcp::ContentBlock::Resource { uri, .. } => {
                        Some(format!("[resource: {uri}]"))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: name.into(),
                content: serde_json::json!({
                    "result": text,
                    "is_error": result.is_error,
                    "server": server_id,
                })
                .to_string(),
                success: !result.is_error,
            }
        }
        Err(error) => tool_error(name, format!("MCP 调用失败: {error}")),
    }
}

#[cfg(test)]
mod mcp_tests {
    use super::parse_mcp_tool_name;

    #[test]
    fn parses_server_qualified_mcp_tool_name() {
        assert_eq!(
            parse_mcp_tool_name("mcp_browser__navigate_page"),
            (Some("browser"), "navigate_page")
        );
        assert_eq!(parse_mcp_tool_name("mcp_search"), (None, "search"));
    }
}

async fn execute_isolated_http_request(
    arguments: &str,
    scope_snapshot: &serde_json::Value,
    session_id: &str,
    run_id: Option<&str>,
    execution_id: Option<&str>,
) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(value) => value,
        Err(error) => return tool_error("http_request", format!("参数解析失败: {error}")),
    };
    let Some(url) = args
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    else {
        return tool_error("http_request", "url 参数为空");
    };
    let method = args
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let Some(bus) = crate::transport::mq::conversation_tool_bus() else {
        return tool_error("http_request", "pent-agent 执行通道不可用");
    };
    let request = crate::transport::mq::RemoteToolExecutionRequest {
        execution_id: execution_id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        tool_name: "http_request".into(),
        arguments: args,
        scope_snapshot: scope_snapshot.clone(),
        session_id: session_id.into(),
        run_id: run_id.map(str::to_owned),
        traffic_run_id: None,
        timeout_seconds: 30,
    };
    let output = match bus.request_tool_execution(&request).await {
        Ok(response) if response.error.is_none() && response.output.is_some() => {
            response.output.expect("checked")
        }
        Ok(response) => {
            return tool_error(
                "http_request",
                response
                    .error
                    .unwrap_or_else(|| "pent-agent 返回空结果".into()),
            );
        }
        Err(error) => return tool_error("http_request", error),
    };
    let csrf_token_captured = extract_csrf_token(&output.stdout).is_some();
    let status = last_http_status(&output.stdout);
    let raw_response =
        truncate_http_response_body(output.stdout.clone(), MAX_HTTP_RESPONSE_BODY_BYTES);
    let success = output.exit_code == 0 && status.is_some_and(|status| status < 500);
    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "http_request".into(),
        content: serde_json::json!({
            "url": url,
            "method": method,
            "status": status,
            "redirects_followed": true,
            "cookie_jar": "session",
            "csrf_token_captured": csrf_token_captured,
            "raw_response": raw_response,
            "stderr": output.stderr,
            "elapsed_ms": output.duration_ms,
            "executor": output.audit
        })
        .to_string(),
        success,
    }
}

async fn execute_isolated_http_probe(
    arguments: &str,
    scope_snapshot: &serde_json::Value,
    session_id: &str,
    run_id: Option<&str>,
    execution_id: Option<&str>,
) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(value) => value,
        Err(error) => return tool_error("http_probe", format!("参数解析失败: {error}")),
    };
    let Some(target) = args.get("target").and_then(serde_json::Value::as_str) else {
        return tool_error("http_probe", "target 参数为空");
    };
    let Some(bus) = crate::transport::mq::conversation_tool_bus() else {
        return tool_error("http_probe", "pent-agent 执行通道不可用");
    };
    let mut results = Vec::new();
    let mut reachable = false;
    for url in http_probe_urls(target) {
        let request = crate::transport::mq::RemoteToolExecutionRequest {
            execution_id: execution_id
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            tool_name: "http_probe".into(),
            arguments: serde_json::json!({"target":url}),
            scope_snapshot: scope_snapshot.clone(),
            session_id: session_id.into(),
            run_id: run_id.map(str::to_owned),
            traffic_run_id: None,
            timeout_seconds: 30,
        };
        match bus.request_tool_execution(&request).await {
            Ok(response) if response.error.is_none() && response.output.is_some() => {
                let output = response.output.expect("checked");
                let status = last_http_status(&output.stdout);
                let effective = output.exit_code == 0 && status.is_some();
                reachable |= effective;
                results.push(serde_json::json!({
                    "url": url,
                    "reachable": effective,
                    "status": status,
                    "raw_response": truncate_http_response_body(
                        output.stdout,
                        MAX_HTTP_RESPONSE_BODY_BYTES
                    ),
                    "stderr": output.stderr,
                    "elapsed_ms": output.duration_ms,
                    "executor": output.audit
                }));
            }
            Ok(response) => results.push(serde_json::json!({
                "url": url,
                "reachable": false,
                "error": response.error.unwrap_or_else(|| "pent-agent 返回空结果".into())
            })),
            Err(error) => results.push(serde_json::json!({
                "url": url,
                "reachable": false,
                "error": error
            })),
        }
    }
    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "http_probe".into(),
        content: serde_json::json!({"results": results}).to_string(),
        success: reachable,
    }
}

pub(crate) async fn http_session_state_dir(session_id: &str) -> Result<std::path::PathBuf, String> {
    let safe_session: String = session_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(64)
        .collect();
    if safe_session.is_empty() {
        return Err("会话 ID 无效".into());
    }
    let root = std::env::var("PENTA_TOOL_STATE_ROOT")
        .unwrap_or_else(|_| "/tmp/kairos-pent-tool-state".into());
    let path = std::path::Path::new(&root).join(safe_session);
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|error| format!("无法创建 HTTP 会话状态目录: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|error| format!("无法保护 HTTP 会话状态目录: {error}"))?;
    }
    Ok(path)
}

fn last_http_status(raw: &str) -> Option<u16> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            let mut parts = line.split_ascii_whitespace();
            parts
                .next()
                .filter(|protocol| protocol.starts_with("HTTP/"))?;
            parts.next()?.parse::<u16>().ok()
        })
        .last()
}

pub(crate) fn extract_csrf_token(raw: &str) -> Option<String> {
    let header = regex::Regex::new(r"(?im)^x-csrf-token:\s*([^\r\n]+)").ok()?;
    if let Some(value) = header.captures(raw).and_then(|capture| capture.get(1)) {
        return Some(value.as_str().trim().to_string());
    }
    let html = regex::Regex::new(
        r#"(?is)(?:name=["'](?:_csrf|csrf_token)["'][^>]*value=["']([^"']+)|content=["']([^"']+)["'][^>]*name=["']csrf-token["'])"#,
    )
    .ok()?;
    html.captures(raw).and_then(|capture| {
        capture
            .get(1)
            .or_else(|| capture.get(2))
            .map(|value| value.as_str().to_string())
    })
}

fn tool_error(tool_name: &str, error: impl ToString) -> ConversationToolResult {
    let message = error.to_string();
    let (error_code, retryable) = classify_tool_error(&message);
    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: tool_name.into(),
        content: serde_json::json!({
            "error": message,
            "error_code": error_code,
            "retryable": retryable,
            "blocked": false
        })
        .to_string(),
        success: false,
    }
}

fn classify_tool_error(error: &str) -> (&'static str, bool) {
    if error.contains("取消") {
        ("TOOL_CANCELLED", false)
    } else if error.contains("超时") {
        ("TOOL_TIMEOUT", true)
    } else if error.contains("没有可用的 pent-agent")
        || error.contains("执行通道不可用")
        || error.contains("响应通道已关闭")
        || error.contains("WORKER_UNAVAILABLE")
    {
        ("WORKER_UNAVAILABLE", true)
    } else if error.contains("参数") {
        ("INVALID_ARGUMENT", false)
    } else if error.contains("未注册") || error.contains("不存在") || error.contains("未安装")
    {
        ("TOOL_UNAVAILABLE", false)
    } else {
        ("TOOL_EXECUTION_FAILED", false)
    }
}

async fn execute_http_request(arguments: &str) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "http_request".into(),
                content: serde_json::json!({"error": format!("参数解析失败: {}", e)}).to_string(),
                success: false,
            };
        }
    };

    let url = args["url"].as_str().unwrap_or("");
    let method = args["method"].as_str().unwrap_or("GET").to_uppercase();
    let body = args["body"].as_str().unwrap_or("");

    if url.is_empty() {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: "http_request".into(),
            content: serde_json::json!({"error": "url 参数为空"}).to_string(),
            success: false,
        };
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .danger_accept_invalid_certs(true)
        .user_agent("Kairos-Pent-Agent/1.0")
        .redirect(reqwest::redirect::Policy::none())
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "http_request".into(),
                content: serde_json::json!({"error": format!("无法创建 HTTP 客户端: {}", e)})
                    .to_string(),
                success: false,
            };
        }
    };

    let mut req = match method.as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "HEAD" => client.head(url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, url),
        "PATCH" => client.patch(url),
        other => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "http_request".into(),
                content: serde_json::json!({"error": format!("不支持的 HTTP 方法: {}", other)})
                    .to_string(),
                success: false,
            };
        }
    };

    // Accept the registry schema's object form and the legacy JSON-string form.
    let headers = match &args["headers"] {
        serde_json::Value::Object(headers) => Some(headers.clone()),
        serde_json::Value::String(headers) => serde_json::from_str(headers)
            .ok()
            .and_then(|value: serde_json::Value| value.as_object().cloned()),
        _ => None,
    };
    let has_content_type = headers.as_ref().is_some_and(|headers| {
        headers
            .keys()
            .any(|key| key.eq_ignore_ascii_case("content-type"))
    });
    if let Some(headers) = headers {
        for (key, value) in headers {
            if let Some(value) = value.as_str() {
                req = req.header(key, value);
            }
        }
    }

    if !body.is_empty() {
        if !has_content_type {
            req = req.header("Content-Type", "application/json");
        }
        req = req.body(body.to_string());
    }

    let started = Instant::now();
    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let resp_headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            let content_length = resp.content_length();
            let body_text = match resp.text().await {
                Ok(t) => truncate_http_response_body(t, MAX_HTTP_RESPONSE_BODY_BYTES),
                Err(e) => format!("(读取响应体失败: {})", e),
            };

            let elapsed_ms = started.elapsed().as_millis() as u64;
            let result = serde_json::json!({
                "status": status,
                "headers": resp_headers,
                "content_length": content_length,
                "body": body_text,
                "elapsed_ms": elapsed_ms,
                "analysis_hint": analyze_http_response(status, &body_text, &resp_headers)
            });

            ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "http_request".into(),
                content: result.to_string(),
                success: status < 500,
            }
        }
        Err(e) => ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: "http_request".into(),
            content: serde_json::json!({
                "error": format!("HTTP 请求失败: {}", e),
                "url": url,
                "method": method
            })
            .to_string(),
            success: false,
        },
    }
}

async fn execute_http_probe(arguments: &str) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "http_probe".into(),
                content: serde_json::json!({"error": format!("参数解析失败: {}", e)}).to_string(),
                success: false,
            };
        }
    };

    let target = args["target"].as_str().unwrap_or("").trim();
    if target.is_empty() {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: "http_probe".into(),
            content: serde_json::json!({"error": "target 参数为空"}).to_string(),
            success: false,
        };
    }

    let mut results = Vec::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .user_agent("Kairos-Pent-Agent/1.0")
        .redirect(reqwest::redirect::Policy::none())
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "http_probe".into(),
                content: serde_json::json!({"error": format!("无法创建 HTTP 客户端: {}", e)})
                    .to_string(),
                success: false,
            };
        }
    };

    for url in http_probe_urls(target) {
        let started = Instant::now();
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let server = resp
                    .headers()
                    .get("server")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let body_sample = match resp.text().await {
                    Ok(t) => t.chars().take(2000).collect::<String>(),
                    Err(_) => String::new(),
                };
                let title = extract_title(&body_sample);

                results.push(serde_json::json!({
                    "url": url,
                    "scheme": url.split_once("://").map(|(scheme, _)| scheme).unwrap_or(""),
                    "status": status,
                    "server": server,
                    "content_type": content_type,
                    "title": title,
                    "body_preview": body_sample.chars().take(500).collect::<String>(),
                    "elapsed_ms": started.elapsed().as_millis() as u64
                }));
            }
            Err(_) => {
                results.push(serde_json::json!({
                    "url": url,
                    "scheme": url.split_once("://").map(|(scheme, _)| scheme).unwrap_or(""),
                    "reachable": false
                }));
            }
        }
    }

    let reachable = results.iter().any(|result| {
        result
            .get("status")
            .and_then(serde_json::Value::as_u64)
            .is_some()
    });
    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "http_probe".into(),
        content: serde_json::json!({"results": results}).to_string(),
        success: reachable,
    }
}

async fn execute_dns_lookup(arguments: &str) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(e) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "dns_lookup".into(),
                content: serde_json::json!({"error": format!("参数解析失败: {}", e)}).to_string(),
                success: false,
            };
        }
    };

    let domain = args["domain"]
        .as_str()
        .or(args["target"].as_str())
        .unwrap_or("");
    if domain.is_empty() {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: "dns_lookup".into(),
            content: serde_json::json!({"error": "domain/target 参数为空"}).to_string(),
            success: false,
        };
    }

    let mut result = serde_json::json!({"domain": domain, "records": {}});

    // Try A records via system resolver
    if let Ok(addrs) = tokio::net::lookup_host(format!("{}:0", domain)).await {
        let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
        if !ips.is_empty() {
            result["records"]["A"] = serde_json::json!(ips);
        }
    }

    // Try to resolve via HTTPS DNS (Google DNS-over-HTTPS as fallback)
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "dns_lookup".into(),
                content: result.to_string(),
                success: result["records"]["A"].is_array(),
            };
        }
    };

    // A record
    if let Ok(resp) = client
        .get(&format!(
            "https://dns.google/resolve?name={}&type=A",
            domain
        ))
        .send()
        .await
    {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(answers) = body["Answer"].as_array() {
                let ips: Vec<String> = answers
                    .iter()
                    .filter_map(|a| a["data"].as_str().map(str::to_string))
                    .collect();
                if !ips.is_empty() {
                    result["records"]["A"] = serde_json::json!(ips);
                }
            }
        }
    }

    // CNAME
    if let Ok(resp) = client
        .get(&format!(
            "https://dns.google/resolve?name={}&type=CNAME",
            domain
        ))
        .send()
        .await
    {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(answers) = body["Answer"].as_array() {
                let cnames: Vec<String> = answers
                    .iter()
                    .filter_map(|a| {
                        a["data"]
                            .as_str()
                            .map(|s| s.trim_end_matches('.').to_string())
                    })
                    .collect();
                if !cnames.is_empty() {
                    result["records"]["CNAME"] = serde_json::json!(cnames);
                }
            }
        }
    }

    // MX
    if let Ok(resp) = client
        .get(&format!(
            "https://dns.google/resolve?name={}&type=MX",
            domain
        ))
        .send()
        .await
    {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(answers) = body["Answer"].as_array() {
                let mx: Vec<String> = answers
                    .iter()
                    .filter_map(|a| a["data"].as_str().map(str::to_string))
                    .collect();
                if !mx.is_empty() {
                    result["records"]["MX"] = serde_json::json!(mx);
                }
            }
        }
    }

    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "dns_lookup".into(),
        content: result.to_string(),
        success: result["records"]["A"].is_array(),
    }
}

async fn execute_web_crawl(arguments: &str) -> ConversationToolResult {
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(_) => serde_json::json!({}),
    };
    let url = args["url"].as_str().unwrap_or("");
    if url.is_empty() {
        return ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: "web_url_collect".into(),
            content: serde_json::json!({"error": "url 参数为空"}).to_string(),
            success: false,
        };
    }

    // Make a GET request and extract URLs from the response
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .user_agent("Kairos-Pent-Agent/1.0")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "web_url_collect".into(),
                content: serde_json::json!({"error": format!("无法创建客户端: {}", e)}).to_string(),
                success: false,
            };
        }
    };

    match client.get(url).send().await {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            let urls = extract_urls_from_html(&body);
            let js_files: Vec<&String> = urls.iter().filter(|u| u.ends_with(".js")).collect();

            ConversationToolResult {
                tool_call_id: String::new(),
                tool_name: "web_url_collect".into(),
                content: serde_json::json!({
                    "url": url,
                    "total_urls_found": urls.len(),
                    "js_files_found": js_files.len(),
                    "urls": urls.iter().take(200).collect::<Vec<_>>(),
                    "js_files": js_files.iter().take(100).collect::<Vec<_>>(),
                })
                .to_string(),
                success: true,
            }
        }
        Err(e) => ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: "web_url_collect".into(),
            content: serde_json::json!({"error": format!("请求失败: {}", e)}).to_string(),
            success: false,
        },
    }
}

// ── helpers ────────────────────────────────────────────────────

fn truncate_http_response_body(body: String, max_bytes: usize) -> String {
    if body.len() <= max_bytes {
        return body;
    }

    let mut end = max_bytes;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...(truncated, total {} bytes)", &body[..end], body.len())
}

fn extract_urls_from_html(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    // Extract href/src attributes
    for attr in ["href=\"", "href='", "src=\"", "src='"] {
        let mut remaining: &str = html;
        while let Some(pos) = remaining.find(attr) {
            let start = pos + attr.len();
            if let Some(end) = remaining[start..].find(if attr.contains('"') { '"' } else { '\'' })
            {
                let url = &remaining[start..start + end];
                if !urls.contains(&url.to_string()) {
                    urls.push(url.to_string());
                }
                remaining = &remaining[start + end + 1..];
            } else {
                break;
            }
        }
    }
    // Extract JS file references
    let mut remaining = html;
    while let Some(pos) = remaining.find(".js") {
        // Walk backwards to find the start of the path
        let before = &remaining[..pos + 3];
        let mut start = before.len();
        for delim in ['"', '\'', ' ', '>', ')', '`'] {
            if let Some(p) = before.rfind(delim) {
                start = start.min(p + 1);
            }
        }
        // Walk forwards to find end if it's quoted
        let after = &remaining[pos + 3..];
        let end = after
            .find(|c| c == '"' || c == '\'' || c == ' ' || c == '<' || c == ')')
            .map(|e| pos + 3 + e)
            .unwrap_or(pos + 3);
        let url = remaining[start..end].to_string();
        if url.len() > 3 && !urls.contains(&url) {
            urls.push(url);
        }
        remaining = &remaining[pos + 3..];
        if remaining.is_empty() {
            break;
        }
    }
    urls
}

fn extract_title(html: &str) -> String {
    let lower = html.to_lowercase();
    if let Some(start) = lower.find("<title") {
        if let Some(content_start) = lower[start..].find('>') {
            let content = &html[start + content_start + 1..];
            if let Some(end) = content.to_lowercase().find("</title>") {
                return content[..end].trim().to_string();
            }
        }
    }
    String::new()
}

fn http_probe_urls(target: &str) -> Vec<String> {
    let target = target.trim().trim_end_matches('/');
    if target.starts_with("http://") || target.starts_with("https://") {
        vec![target.to_string()]
    } else {
        vec![format!("https://{target}"), format!("http://{target}")]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_tool_error, extract_csrf_token, http_probe_urls, last_http_status, tool_error,
        truncate_http_response_body,
    };

    #[test]
    fn remote_failures_have_stable_non_policy_error_codes() {
        assert_eq!(
            classify_tool_error("工具执行已取消"),
            ("TOOL_CANCELLED", false)
        );
        assert_eq!(
            classify_tool_error("等待 pent-agent 执行工具超时"),
            ("TOOL_TIMEOUT", true)
        );
        assert_eq!(
            classify_tool_error("没有可用的 pent-agent 工具执行节点"),
            ("WORKER_UNAVAILABLE", true)
        );
        let result = tool_error("nmap", "没有可用的 pent-agent 工具执行节点");
        let content: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(content["blocked"], false);
        assert_eq!(content["error_code"], "WORKER_UNAVAILABLE");
    }

    #[test]
    fn truncates_http_response_body_at_utf8_boundary() {
        let body = format!("{}中文", "a".repeat(49_999));

        let truncated = truncate_http_response_body(body, 50_000);

        assert!(truncated.starts_with(&"a".repeat(49_999)));
        assert!(truncated.ends_with("...(truncated, total 50005 bytes)"));
    }

    #[test]
    fn probe_preserves_an_explicit_scheme() {
        assert_eq!(
            http_probe_urls("https://example.test/"),
            vec!["https://example.test"]
        );
    }

    #[test]
    fn probe_adds_schemes_only_for_bare_targets() {
        assert_eq!(
            http_probe_urls("example.test"),
            vec!["https://example.test", "http://example.test"]
        );
    }

    #[test]
    fn parses_last_status_after_redirects() {
        let raw = "HTTP/1.1 302 Found\r\nLocation: /login\r\n\r\n\
                   HTTP/2 200\r\nContent-Type: text/html\r\n\r\nok";
        assert_eq!(last_http_status(raw), Some(200));
    }

    #[test]
    fn extracts_csrf_from_header_and_form() {
        assert_eq!(
            extract_csrf_token("HTTP/1.1 200 OK\r\nX-CSRF-Token: abc123\r\n"),
            Some("abc123".into())
        );
        assert_eq!(
            extract_csrf_token(r#"<input type="hidden" name="_csrf" value="form-token">"#),
            Some("form-token".into())
        );
    }
}

fn analyze_http_response(status: u16, body: &str, headers: &[(String, String)]) -> String {
    let mut hints: Vec<String> = Vec::new();
    match status {
        200 => hints.push("请求成功，检查响应中是否有敏感信息泄露".to_string()),
        301 | 302 => hints.push("重定向 — 跟踪目标地址".to_string()),
        401 => hints.push("需要认证 — 可能存在认证绕过或弱口令".to_string()),
        403 => hints.push("禁止访问 — 尝试路径/方法/Header 绕过".to_string()),
        404 => hints.push("资源不存在 — 可能路径错误或端点已下线".to_string()),
        405 => hints.push("方法不允许 — 尝试其他 HTTP 方法".to_string()),
        500 => hints.push("服务器错误 — 可能存在注入点或配置错误".to_string()),
        _ => {}
    }
    // Check for common exposures
    if body.contains("API_KEY") || body.contains("apiKey") || body.contains("secret") {
        hints.push("⚠ 响应中可能包含密钥/凭据!".to_string());
    }
    if body.contains("SQL") || body.contains("syntax error") {
        hints.push("⚠ 可能暴露了 SQL 错误信息".to_string());
    }
    if body.contains("Stacktrace") || body.contains("stack trace") {
        hints.push("⚠ 响应中可能包含堆栈跟踪".to_string());
    }
    for (k, v) in headers {
        if k.to_lowercase().contains("server") {
            hints.push(format!("Server 头: {} — 可用于指纹识别", v));
        }
        if k.to_lowercase().contains("x-powered-by") {
            hints.push(format!("X-Powered-By: {} — 暴露了技术栈", v));
        }
    }
    if hints.is_empty() {
        "无明显安全线索".into()
    } else {
        hints.join("; ")
    }
}

// ── Parallel tool execution ──────────────────────────────────────────────

/// Execute multiple tool calls in parallel using tokio::spawn. Each tool gets
/// its own timeout. Results are returned in the same order as the input calls.
/// Enabled when `PENTA_PARALLEL_TOOLS` is not `"false"` (default: enabled).
pub async fn execute_tools_parallel(
    tool_calls: &[(String, String, String)], // (call_id, name, arguments_json)
    scope_snapshot: &serde_json::Value,
    session_id: &str,
    run_id: Option<&str>,
) -> Vec<ConversationToolResult> {
    let handles: Vec<_> = tool_calls
        .iter()
        .enumerate()
        .map(|(idx, (call_id, name, args))| {
            let cid = call_id.clone();
            let n = name.clone();
            let a = args.clone();
            let scope = scope_snapshot.clone();
            let sid = session_id.to_string();
            let rid = run_id.map(str::to_string);
            tokio::spawn(async move {
                let result = execute_tool(&n, &a, &scope, &sid, rid.as_deref(), None).await;
                (idx, cid, result)
            })
        })
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok((idx, cid, result)) => {
                let mut r = result;
                r.tool_call_id = cid;
                results.push((idx, r));
            }
            Err(e) => {
                tracing::warn!(error = %e, "parallel tool task panicked");
            }
        }
    }
    results.sort_by_key(|(idx, _)| *idx);
    results.into_iter().map(|(_, r)| r).collect()
}
