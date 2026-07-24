use futures_util::{StreamExt, stream};
use serde_json::{Value, json};

use super::tool_executor::ConversationToolResult;

const DEFAULT_UPLOAD_MARKER: &str = "KAIROS_PENT_UPLOAD_MARKER";
const MAX_COLLECTED_SCRIPTS: usize = 50;
const MAX_SCRIPT_BYTES: usize = 200_000;
const DEFAULT_DISCOVERY_PATHS: &[&str] = &[
    "admin/",
    "api/",
    "assets/",
    "editor/",
    "file_manager_json.jsp",
    "kindeditor/",
    "kindeditor4.1.11/",
    "kindeditor4.1.11/file_manager_json.jsp",
    "kindeditor4.1.11/upload_image.jsp",
    "kindeditor4.1.11/upload_json.jsp",
    "login",
    "login.jsp",
    "robots.txt",
    "sitemap.xml",
    "upload/",
    "uploads/",
];

pub(super) async fn execute_js_collect(arguments: &str) -> ConversationToolResult {
    let args = parse_arguments("js_collect", arguments);
    let Ok(args) = args else {
        return args.unwrap_err();
    };
    let url = args.get("url").and_then(Value::as_str).unwrap_or("");
    let client = match http_client(15) {
        Ok(client) => client,
        Err(error) => return tool_error("js_collect", format!("客户端创建失败: {error}")),
    };
    let html = if let Some(html) = args.get("html").and_then(Value::as_str) {
        html.to_string()
    } else {
        let url = match reqwest::Url::parse(url) {
            Ok(url) => url,
            Err(error) => return tool_error("js_collect", format!("URL 无效: {error}")),
        };
        match client.get(url).send().await {
            Ok(response) => response.text().await.unwrap_or_default(),
            Err(error) => return tool_error("js_collect", format!("页面获取失败: {error}")),
        }
    };

    let base_url = reqwest::Url::parse(url).ok();
    let mut external = Vec::new();
    let mut inline_count = 0usize;
    let lower = html.to_ascii_lowercase();
    let mut offset = 0usize;
    while let Some(relative_start) = lower[offset..].find("<script") {
        let start = offset + relative_start;
        let Some(tag_end_rel) = lower[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_end_rel;
        let tag = &html[start..=tag_end];
        if let Some(src) = attribute_value(tag, "src") {
            let resolved = base_url
                .as_ref()
                .and_then(|base| base.join(&src).ok())
                .map(|url| url.to_string())
                .unwrap_or(src);
            if !external.contains(&resolved) {
                external.push(resolved);
            }
        } else {
            inline_count += 1;
        }
        offset = tag_end + 1;
    }

    // Fetch only same-origin JavaScript discovered from the authorized page.
    // Third-party script URLs remain visible in external_scripts but are not
    // contacted implicitly.
    let mut collected_scripts = Vec::new();
    for script_url in external.iter().take(MAX_COLLECTED_SCRIPTS) {
        if !same_origin(url, script_url) {
            collected_scripts.push(json!({
                "url": script_url,
                "downloaded": false,
                "reason": "cross_origin"
            }));
            continue;
        }
        let collected = match client.get(script_url).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                match response.bytes().await {
                    Ok(bytes) => {
                        let truncated = bytes.len() > MAX_SCRIPT_BYTES;
                        let end = bytes.len().min(MAX_SCRIPT_BYTES);
                        json!({
                            "url": script_url,
                            "downloaded": true,
                            "status": status,
                            "bytes": bytes.len(),
                            "truncated": truncated,
                            "content": String::from_utf8_lossy(&bytes[..end])
                        })
                    }
                    Err(error) => json!({
                        "url": script_url,
                        "downloaded": false,
                        "status": status,
                        "error": error.to_string()
                    }),
                }
            }
            Err(error) => json!({
                "url": script_url,
                "downloaded": false,
                "error": error.to_string()
            }),
        };
        collected_scripts.push(collected);
    }

    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "js_collect".into(),
        content: json!({
            "page_url": url,
            "external_scripts": external,
            "external_count": external.len(),
            "inline_script_count": inline_count,
            "collected_scripts": collected_scripts,
            "collection_limit": MAX_COLLECTED_SCRIPTS
        })
        .to_string(),
        success: true,
    }
}

pub(super) fn execute_js_analyze(arguments: &str) -> ConversationToolResult {
    let args = parse_arguments("js_analyze", arguments);
    let Ok(args) = args else {
        return args.unwrap_err();
    };
    let content = args.get("content").and_then(Value::as_str).unwrap_or("");
    if content.trim().is_empty() {
        return tool_error("js_analyze", "content 不能为空");
    }

    let mut endpoints = Vec::new();
    for candidate in quoted_values(content) {
        let lower = candidate.to_ascii_lowercase();
        let looks_like_endpoint = candidate.starts_with('/')
            || candidate.starts_with("http://")
            || candidate.starts_with("https://")
            || [
                ".action", ".asp", ".aspx", ".do", ".jsp", ".php", "/api", "upload",
            ]
            .iter()
            .any(|needle| lower.contains(needle));
        if looks_like_endpoint && candidate.len() <= 500 && !endpoints.contains(&candidate) {
            endpoints.push(candidate);
        }
    }

    let lower = content.to_ascii_lowercase();
    let technology_hints = [
        ("kindeditor", "KindEditor"),
        ("ckeditor", "CKEditor"),
        ("ueditor", "UEditor"),
        ("tinymce", "TinyMCE"),
        ("jquery", "jQuery"),
    ]
    .into_iter()
    .filter_map(|(needle, label)| lower.contains(needle).then_some(label))
    .collect::<Vec<_>>();
    let security_hints = [
        ("authorization", "Authorization header"),
        ("bearer ", "Bearer token"),
        ("localstorage", "localStorage"),
        ("upload", "upload flow"),
        ("token", "token handling"),
    ]
    .into_iter()
    .filter_map(|(needle, label)| lower.contains(needle).then_some(label))
    .collect::<Vec<_>>();

    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "js_analyze".into(),
        content: json!({
            "source_url": args.get("url"),
            "endpoints": endpoints.into_iter().take(300).collect::<Vec<_>>(),
            "technology_hints": technology_hints,
            "security_hints": security_hints
        })
        .to_string(),
        success: true,
    }
}

pub(super) async fn execute_dir_fuzz(arguments: &str) -> ConversationToolResult {
    let args = parse_arguments("dir_fuzz", arguments);
    let Ok(args) = args else {
        return args.unwrap_err();
    };
    let url = args.get("url").and_then(Value::as_str).unwrap_or("");
    let base = match origin_url(url) {
        Ok(base) => base,
        Err(error) => return tool_error("dir_fuzz", error),
    };
    let client = match http_client(8) {
        Ok(client) => client,
        Err(error) => return tool_error("dir_fuzz", error.to_string()),
    };

    let paths = DEFAULT_DISCOVERY_PATHS
        .iter()
        .map(|path| path.to_string())
        .collect::<Vec<_>>();
    let results = stream::iter(paths)
        .map(|path| {
            let client = client.clone();
            let target = base.join(&path);
            async move {
                let target = target.ok()?;
                let response = client.get(target.clone()).send().await.ok()?;
                let status = response.status().as_u16();
                if status == 404 {
                    return None;
                }
                Some(json!({
                    "path": path,
                    "url": target.to_string(),
                    "status": status,
                    "content_type": response.headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("")
                }))
            }
        })
        .buffer_unordered(6)
        .filter_map(|result| async move { result })
        .collect::<Vec<_>>()
        .await;

    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "dir_fuzz".into(),
        content: json!({
            "base_url": base.to_string(),
            "tested_paths": DEFAULT_DISCOVERY_PATHS.len(),
            "discoveries": results
        })
        .to_string(),
        success: true,
    }
}

pub(super) async fn execute_file_upload_validate(arguments: &str) -> ConversationToolResult {
    let args = parse_arguments("file_upload_validate", arguments);
    let Ok(args) = args else {
        return args.unwrap_err();
    };
    let url = args.get("url").and_then(Value::as_str).unwrap_or("");
    let field_name = args
        .get("field_name")
        .and_then(Value::as_str)
        .unwrap_or("imgFile");
    let filename = args
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("kairos-upload-marker.txt");
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "png" | "svg" | "txt") {
        return tool_error(
            "file_upload_validate",
            "标记文件仅允许 txt、png 或 svg 扩展名",
        );
    }
    let content_type = args
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or(match extension.as_str() {
            "png" => "image/png",
            "svg" => "image/svg+xml",
            _ => "text/plain",
        });
    let marker_content = if extension == "svg" {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>{DEFAULT_UPLOAD_MARKER}</text></svg>"
        )
    } else {
        DEFAULT_UPLOAD_MARKER.to_string()
    };
    let part = match reqwest::multipart::Part::bytes(marker_content.into_bytes())
        .file_name(filename.to_string())
        .mime_str(content_type)
    {
        Ok(part) => part,
        Err(error) => return tool_error("file_upload_validate", format!("MIME 无效: {error}")),
    };
    let form = reqwest::multipart::Form::new().part(field_name.to_string(), part);
    let client = match http_client(20) {
        Ok(client) => client,
        Err(error) => return tool_error("file_upload_validate", error.to_string()),
    };
    let started = std::time::Instant::now();
    let response = match client.post(url).multipart(form).send().await {
        Ok(response) => response,
        Err(error) => return tool_error("file_upload_validate", format!("上传请求失败: {error}")),
    };
    let status = response.status().as_u16();
    let response_body = response.text().await.unwrap_or_default();
    let mut verification = Value::Null;

    if let Some(verify_url) = args.get("verify_url").and_then(Value::as_str) {
        if !same_origin(url, verify_url) {
            return tool_error("file_upload_validate", "verify_url 必须与上传端点同源");
        }
        verification = match client.get(verify_url).send().await {
            Ok(response) => {
                let verify_status = response.status().as_u16();
                let verify_content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let body = response.text().await.unwrap_or_default();
                json!({
                    "url": verify_url,
                    "status": verify_status,
                    "content_type": verify_content_type,
                    "marker_found": body.contains(DEFAULT_UPLOAD_MARKER)
                })
            }
            Err(error) => json!({"url": verify_url, "error": error.to_string()}),
        };
    }

    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: "file_upload_validate".into(),
        content: json!({
            "upload": {
                "url": url,
                "status": status,
                "accepted": (200..300).contains(&status),
                "response_preview": response_body.chars().take(2000).collect::<String>()
            },
            "verify": verification,
            "assessment": {
                "storage_confirmed": verification.get("marker_found").and_then(Value::as_bool) == Some(true),
                "impact_validated": false,
                "note": "仅使用无害标记文件；文件被接受或可访问本身不等于漏洞"
            },
            "duration_ms": started.elapsed().as_millis()
        })
        .to_string(),
        success: (200..300).contains(&status),
    }
}

fn parse_arguments(tool_name: &str, arguments: &str) -> Result<Value, ConversationToolResult> {
    serde_json::from_str(arguments)
        .map_err(|error| tool_error(tool_name, format!("参数解析失败: {error}")))
}

fn tool_error(tool_name: &str, error: impl Into<String>) -> ConversationToolResult {
    ConversationToolResult {
        tool_call_id: String::new(),
        tool_name: tool_name.into(),
        content: json!({"error": error.into()}).to_string(),
        success: false,
    }
}

fn http_client(timeout_seconds: u64) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds))
        .danger_accept_invalid_certs(true)
        .user_agent("Kairos-Pent-Agent/1.0")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
}

fn origin_url(url: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|error| format!("URL 无效: {error}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL 缺少主机".to_string())?;
    let origin = match parsed.port() {
        Some(port) => format!("{}://{}:{port}/", parsed.scheme(), host),
        None => format!("{}://{host}/", parsed.scheme()),
    };
    reqwest::Url::parse(&origin).map_err(|error| format!("URL 无效: {error}"))
}

fn same_origin(left: &str, right: &str) -> bool {
    match (reqwest::Url::parse(left), reqwest::Url::parse(right)) {
        (Ok(left), Ok(right)) => {
            left.scheme() == right.scheme()
                && left.host_str() == right.host_str()
                && left.port_or_known_default() == right.port_or_known_default()
        }
        _ => false,
    }
}

fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let start = lower.find(&format!("{attribute}="))? + attribute.len() + 1;
    let value = tag[start..].trim_start();
    let quote = value.chars().next()?;
    if quote == '"' || quote == '\'' {
        let rest = &value[quote.len_utf8()..];
        return rest.find(quote).map(|end| rest[..end].to_string());
    }
    Some(
        value
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('>')
            .to_string(),
    )
}

fn quoted_values(content: &str) -> Vec<String> {
    let mut values = Vec::new();
    for quote in ['"', '\'', '`'] {
        let mut rest = content;
        while let Some(start) = rest.find(quote) {
            rest = &rest[start + quote.len_utf8()..];
            let Some(end) = rest.find(quote) else {
                break;
            };
            let value = rest[..end].trim();
            if !value.is_empty() {
                values.push(value.to_string());
            }
            rest = &rest[end + quote.len_utf8()..];
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        http::{StatusCode, header},
        response::IntoResponse,
        routing::get,
    };
    use serde_json::{Value, json};

    use super::{DEFAULT_UPLOAD_MARKER, attribute_value, execute_js_analyze, same_origin};
    use crate::conversation::tool_executor::execute_tool;

    #[test]
    fn extracts_script_src_attribute() {
        assert_eq!(
            attribute_value(r#"<script defer src="/assets/app.js">"#, "src"),
            Some("/assets/app.js".into())
        );
    }

    #[test]
    fn js_analysis_finds_editor_and_upload_routes() {
        let result = execute_js_analyze(
            r#"{"url":"https://example.test/app.js","content":"window.editor='kindeditor'; const upload='/kindeditor/upload_image.jsp';"}"#,
        );
        assert!(result.success);
        assert!(result.content.contains("KindEditor"));
        assert!(result.content.contains("upload_image.jsp"));
    }

    #[test]
    fn upload_verification_must_remain_same_origin() {
        assert!(same_origin(
            "https://example.test/upload",
            "https://example.test/files/marker.txt"
        ));
        assert!(!same_origin(
            "https://example.test/upload",
            "https://other.test/files/marker.txt"
        ));
    }

    #[tokio::test]
    async fn local_fixture_covers_js_discovery_and_harmless_upload_validation() {
        async fn index() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, "text/html")],
                r#"<html><script src="/assets/app.js"></script></html>"#,
            )
        }
        async fn app_js() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, "application/javascript")],
                "window.editor='kindeditor'; const upload='/kindeditor4.1.11/upload_json.jsp';",
            )
        }
        async fn upload() -> impl IntoResponse {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"error":0,"url":"/uploads/kairos-upload-marker.txt"}"#,
            )
        }
        async fn uploaded_marker() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, "text/plain")],
                DEFAULT_UPLOAD_MARKER,
            )
        }

        let app = Router::new()
            .route("/", get(index))
            .route("/assets/app.js", get(app_js))
            .route(
                "/kindeditor4.1.11/upload_json.jsp",
                get(|| async { StatusCode::METHOD_NOT_ALLOWED }).post(upload),
            )
            .route("/uploads/kairos-upload-marker.txt", get(uploaded_marker));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let base_url = format!("http://{address}");
        let scope = json!({
            "targets": [base_url],
            "confirmed_at": "2026-07-24T00:00:00Z"
        });

        let js = execute_tool(
            "js_collect",
            &json!({"url": format!("{base_url}/")}).to_string(),
            &scope,
        )
        .await;
        assert!(js.success, "{}", js.content);
        let js_content: Value = serde_json::from_str(&js.content).unwrap();
        assert_eq!(js_content["external_count"], 1);
        assert_eq!(
            js_content["external_scripts"][0],
            format!("{base_url}/assets/app.js")
        );
        assert_eq!(js_content["collected_scripts"][0]["downloaded"], true);
        assert!(
            js_content["collected_scripts"][0]["content"]
                .as_str()
                .unwrap()
                .contains("kindeditor")
        );

        let discovery = execute_tool(
            "dir_fuzz",
            &json!({"url": format!("{base_url}/")}).to_string(),
            &scope,
        )
        .await;
        assert!(discovery.success, "{}", discovery.content);
        let discovery_content: Value = serde_json::from_str(&discovery.content).unwrap();
        assert!(
            discovery_content["discoveries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["path"] == "kindeditor4.1.11/upload_json.jsp")
        );

        let upload = execute_tool(
            "file_upload_validate",
            &json!({
                "url": format!("{base_url}/kindeditor4.1.11/upload_json.jsp"),
                "verify_url": format!("{base_url}/uploads/kairos-upload-marker.txt"),
                "confirm_authorized": true,
                "filename": "kairos-upload-marker.txt"
            })
            .to_string(),
            &scope,
        )
        .await;
        assert!(upload.success, "{}", upload.content);
        let upload_content: Value = serde_json::from_str(&upload.content).unwrap();
        assert_eq!(upload_content["verify"]["marker_found"], true);
        assert_eq!(upload_content["assessment"]["storage_confirmed"], true);
        assert_eq!(upload_content["assessment"]["impact_validated"], false);

        server.abort();
    }
}
