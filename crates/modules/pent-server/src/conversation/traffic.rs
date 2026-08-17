//! HTTP traffic evidence helpers — pure parsing / diffing logic shared by the
//! conversation runtime and the Postgres repository. No I/O here.

use serde_json::{Value, json};

/// One HTTP request/response exchange ready to persist.
#[derive(Debug, Clone)]
pub struct TrafficDraft {
    pub method: String,
    pub url: String,
    pub request_headers: Value,
    pub request_body: Option<String>,
    pub response_status: Option<i32>,
    pub response_headers: Value,
    pub response_body: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Build traffic drafts from a conversation tool call. `content` is the JSON
/// result returned to the model. Returns an empty vec when the tool produced
/// no parseable exchange (e.g. policy-blocked calls carry no request data).
pub fn drafts_from_tool_result(
    tool_name: &str,
    arguments: &Value,
    content: &str,
) -> Vec<TrafficDraft> {
    let Ok(result) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    match tool_name {
        "http_request" => {
            let Some(url) = result
                .get("url")
                .and_then(Value::as_str)
                .filter(|url| !url.is_empty())
            else {
                return Vec::new();
            };
            let raw = result
                .get("raw_response")
                .and_then(Value::as_str)
                .unwrap_or("");
            let (response_headers, response_body) = parse_raw_response(raw);
            vec![TrafficDraft {
                method: result
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .to_ascii_uppercase(),
                url: url.to_string(),
                request_headers: arguments
                    .get("headers")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
                request_body: arguments
                    .get("body")
                    .and_then(Value::as_str)
                    .filter(|body| !body.is_empty())
                    .map(str::to_owned),
                response_status: result
                    .get("status")
                    .and_then(Value::as_i64)
                    .map(|status| i32::try_from(status).unwrap_or(i32::MAX)),
                response_headers,
                response_body,
                duration_ms: result.get("elapsed_ms").and_then(Value::as_i64),
            }]
        }
        "http_probe" => result
            .get("results")
            .and_then(Value::as_array)
            .map(|results| {
                results
                    .iter()
                    .filter_map(|entry| {
                        let url = entry.get("url").and_then(Value::as_str)?;
                        if url.is_empty() {
                            return None;
                        }
                        let raw = entry
                            .get("raw_response")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let (response_headers, response_body) = parse_raw_response(raw);
                        Some(TrafficDraft {
                            method: "GET".into(),
                            url: url.to_string(),
                            request_headers: json!({}),
                            request_body: None,
                            response_status: entry
                                .get("status")
                                .and_then(Value::as_i64)
                                .map(|status| i32::try_from(status).unwrap_or(i32::MAX)),
                            response_headers,
                            response_body,
                            duration_ms: entry.get("elapsed_ms").and_then(Value::as_i64),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        "http-framework-test" => {
            let Some(url) = arguments
                .get("url")
                .and_then(Value::as_str)
                .filter(|url| !url.is_empty())
            else {
                return Vec::new();
            };
            let raw = result
                .get("stdout")
                .or_else(|| result.get("raw_response"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if raw.is_empty() {
                return Vec::new();
            }
            let (response_headers, response_body) = parse_raw_response(raw);
            let mut request_headers = serde_json::Map::new();
            if let Some(header) = arguments.get("headers").and_then(Value::as_str) {
                if let Some((name, value)) = header.split_once(':') {
                    request_headers.insert(name.trim().to_ascii_lowercase(), json!(value.trim()));
                }
            }
            if let Some(cookies) = arguments.get("cookies").and_then(Value::as_str) {
                request_headers.insert("cookie".into(), json!(cookies));
            }
            vec![TrafficDraft {
                method: arguments
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .to_ascii_uppercase(),
                url: url.to_string(),
                request_headers: Value::Object(request_headers),
                request_body: arguments
                    .get("data")
                    .and_then(Value::as_str)
                    .filter(|body| !body.is_empty())
                    .map(str::to_owned),
                response_status: response_status(raw),
                response_headers,
                response_body,
                duration_ms: result
                    .get("duration_ms")
                    .or_else(|| result.get("elapsed_ms"))
                    .and_then(Value::as_i64),
            }]
        }
        _ => Vec::new(),
    }
}

fn response_status(raw: &str) -> Option<i32> {
    raw.lines()
        .filter(|line| line.trim_start().starts_with("HTTP/"))
        .filter_map(|line| line.split_whitespace().nth(1)?.parse::<i32>().ok())
        .last()
}

/// Parse `curl -i` style output: one or more `HTTP/...` header blocks
/// (redirect chains, 100-continue) followed by the final body. The last
/// header block wins; the body is everything after its blank line.
pub fn parse_raw_response(raw: &str) -> (Value, Option<String>) {
    let mut rest = raw;
    let mut headers = serde_json::Map::new();
    loop {
        let first_line_end = rest.find('\n').unwrap_or(rest.len());
        let first_line = rest[..first_line_end].trim_end();
        if !first_line.starts_with("HTTP/") {
            break;
        }
        let Some(split_at) = find_blank_line(rest) else {
            break;
        };
        let header_block = &rest[..split_at.0];
        // A new header block starts: only the final response's headers matter
        // (redirect chains, 100-continue), so drop any earlier block's values.
        headers.clear();
        for line in header_block.lines().skip(1) {
            let line = line.trim_end_matches('\r');
            if let Some((name, value)) = line.split_once(':') {
                let name = name.trim().to_ascii_lowercase();
                let value = value.trim().to_string();
                headers
                    .entry(name)
                    .and_modify(|existing: &mut Value| {
                        if let Some(previous) = existing.as_str() {
                            *existing = Value::String(format!("{previous}, {value}"));
                        }
                    })
                    .or_insert(Value::String(value));
            }
        }
        rest = &rest[split_at.1..];
    }
    let body = if rest.trim().is_empty() {
        None
    } else {
        Some(rest.to_string())
    };
    (Value::Object(headers), body)
}

/// Returns `(header_block_end, body_start)` around the first blank line.
fn find_blank_line(text: &str) -> Option<(usize, usize)> {
    if let Some(at) = text.find("\r\n\r\n") {
        return Some((at, at + 4));
    }
    text.find("\n\n").map(|at| (at, at + 2))
}

/// Line-level diff summary between two stored responses.
pub fn diff_bodies(a: &str, b: &str) -> Value {
    if a == b {
        return json!({"equal": true, "a_len": a.len(), "b_len": b.len()});
    }
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();
    let mut first_diff = None;
    for index in 0..a_lines.len().max(b_lines.len()) {
        let a_line = a_lines.get(index).copied().unwrap_or("<missing>");
        let b_line = b_lines.get(index).copied().unwrap_or("<missing>");
        if a_line != b_line {
            first_diff = Some(json!({
                "line": index + 1,
                "a": clip_line(a_line),
                "b": clip_line(b_line),
            }));
            break;
        }
    }
    json!({
        "equal": false,
        "a_len": a.len(),
        "b_len": b.len(),
        "a_lines": a_lines.len(),
        "b_lines": b_lines.len(),
        "first_diff": first_diff,
    })
}

fn clip_line(line: &str) -> String {
    const MAX: usize = 200;
    if line.chars().count() <= MAX {
        return line.to_string();
    }
    format!("{}…", line.chars().take(MAX).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_redirect_chain_and_keeps_last_headers() {
        let raw = "HTTP/1.1 302 Found\r\nLocation: /login\r\nSet-Cookie: a=1\r\n\r\n\
                   HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html>ok</html>";
        let (headers, body) = parse_raw_response(raw);
        assert_eq!(headers["content-type"], "text/html");
        assert!(headers.get("location").is_none());
        assert_eq!(body.as_deref(), Some("<html>ok</html>"));
    }

    #[test]
    fn parses_plain_body_without_headers() {
        let (headers, body) = parse_raw_response("not an http response");
        assert_eq!(headers, json!({}));
        assert_eq!(body.as_deref(), Some("not an http response"));
    }

    #[test]
    fn header_only_response_has_no_body() {
        let (headers, body) = parse_raw_response("HTTP/1.1 204 No Content\nX-A: 1\n\n");
        assert_eq!(headers["x-a"], "1");
        assert!(body.is_none());
    }

    #[test]
    fn drafts_single_http_request() {
        let content = json!({
            "url": "http://t/login", "method": "POST", "status": 200,
            "raw_response": "HTTP/1.1 200 OK\nX: 1\n\nbody", "elapsed_ms": 42
        })
        .to_string();
        let args = json!({"headers": {"X-Req": "v"}, "body": "u=a"});
        let drafts = drafts_from_tool_result("http_request", &args, &content);
        assert_eq!(drafts.len(), 1);
        let draft = &drafts[0];
        assert_eq!(draft.method, "POST");
        assert_eq!(draft.response_status, Some(200));
        assert_eq!(draft.request_body.as_deref(), Some("u=a"));
        assert_eq!(draft.response_body.as_deref(), Some("body"));
        assert_eq!(draft.duration_ms, Some(42));
    }

    #[test]
    fn drafts_each_probe_result() {
        let content = json!({
            "results": [
                {"url": "http://t/", "status": 200, "raw_response": "HTTP/1.1 200 OK\n\nroot"},
                {"url": "https://t/", "reachable": false, "error": "timeout"}
            ]
        })
        .to_string();
        let drafts = drafts_from_tool_result("http_probe", &json!({}), &content);
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].response_body.as_deref(), Some("root"));
        assert_eq!(drafts[1].response_status, None);
    }

    #[test]
    fn drafts_structured_framework_request_with_cookie() {
        let content = json!({
            "stdout": "HTTP/1.1 302 Found\r\nLocation: /index.php\r\n\r\n"
        })
        .to_string();
        let args = json!({
            "url": "http://dvwa/login.php",
            "method": "POST",
            "data": "username=admin",
            "headers": "Content-Type: application/x-www-form-urlencoded",
            "cookies": "PHPSESSID=abc; security=low"
        });
        let drafts = drafts_from_tool_result("http-framework-test", &args, &content);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].response_status, Some(302));
        assert_eq!(drafts[0].request_headers["cookie"], "PHPSESSID=abc; security=low");
        assert_eq!(drafts[0].request_headers["content-type"], "application/x-www-form-urlencoded");
        assert_eq!(drafts[0].request_body.as_deref(), Some("username=admin"));
    }

    #[test]
    fn diff_reports_first_diverging_line() {
        let diff = diff_bodies("a\nb\nc", "a\nx\nc");
        assert_eq!(diff["equal"], false);
        assert_eq!(diff["first_diff"]["line"], 2);
        assert_eq!(diff_bodies("same", "same")["equal"], true);
    }

    #[test]
    fn blocked_call_produces_no_draft() {
        let content = json!({"error": "blocked", "blocked": true}).to_string();
        assert!(drafts_from_tool_result("http_request", &json!({}), &content).is_empty());
    }
}
