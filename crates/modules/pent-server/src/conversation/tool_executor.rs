//! Tool executor for the conversation agent.
//! Provides real implementations of basic tools that can be used
//! during conversation-driven penetration testing.
use std::time::Instant;

const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 50_000;

/// Whether this process has a real conversation-mode handler for a registry
/// tool. The database remains the source of truth for which tools are active;
/// this predicate prevents the model from being offered catalog entries whose
/// worker executors are not implemented yet.
pub(super) fn has_conversation_handler(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "dir_fuzz"
            | "dns_lookup"
            | "file_upload_validate"
            | "http_probe"
            | "http_request"
            | "js_analyze"
            | "js_collect"
            | "web_crawl"
            | "web_fingerprint"
            | "web_url_collect"
    )
}

/// Result of executing a single tool call from the LLM.
#[derive(Debug, Clone)]
pub struct ConversationToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String, // JSON string of the result
    pub success: bool,
}

/// Execute a tool call from the LLM. Returns the result that can be
/// fed back to the model in the next turn.
pub async fn execute_tool(
    name: &str,
    arguments: &str,
    scope_snapshot: &serde_json::Value,
) -> ConversationToolResult {
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

        // ── Fallback ────────────────────────────────────────────
        other => ConversationToolResult {
            tool_call_id: String::new(),
            tool_name: other.into(),
            content: serde_json::json!({
                "error": format!("工具 '{}' 在对话模式中不可用，请通过 Run 方式执行", other),
                "suggestion": "如需使用此工具，请创建渗透任务并在 Run 中执行"
            })
            .to_string(),
            success: false,
        },
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
        .redirect(reqwest::redirect::Policy::limited(5))
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

    let target = args["target"].as_str().unwrap_or("");
    let target = target.trim().trim_end_matches('/');
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
        .redirect(reqwest::redirect::Policy::limited(3))
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

    for scheme in &["https", "http"] {
        let url = format!("{}://{}", scheme, target);
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
                    "scheme": *scheme,
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
                    "scheme": *scheme,
                    "reachable": false
                }));
            }
        }
    }

    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "http_probe".into(),
        content: serde_json::json!({"results": results}).to_string(),
        success: true,
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

#[cfg(test)]
mod tests {
    use super::truncate_http_response_body;

    #[test]
    fn truncates_http_response_body_at_utf8_boundary() {
        let body = format!("{}中文", "a".repeat(49_999));

        let truncated = truncate_http_response_body(body, 50_000);

        assert!(truncated.starts_with(&"a".repeat(49_999)));
        assert!(truncated.ends_with("...(truncated, total 50005 bytes)"));
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
