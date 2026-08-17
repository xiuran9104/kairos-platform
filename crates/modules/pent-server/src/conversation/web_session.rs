use std::collections::{BTreeMap, HashMap};

use regex::Regex;
use serde_json::Value;

use super::ModelToolCall;

/// Run-local authenticated Web state learned from governed HTTP observations.
/// Cookies are only injected into tools that explicitly support them; scope
/// validation still runs after injection and remains authoritative.
#[derive(Debug, Default)]
pub(super) struct WebSessionState {
    cookies: BTreeMap<String, String>,
    csrf_by_path: HashMap<String, (String, String)>,
}

impl WebSessionState {
    pub(super) fn observe(&mut self, arguments: &str, content: &str) {
        let arguments = serde_json::from_str::<Value>(arguments).unwrap_or(Value::Null);
        self.observe_request_auth(&arguments);
        let path = request_path(&arguments).unwrap_or_else(|| "/".into());
        let Ok(content) = serde_json::from_str::<Value>(content) else {
            return;
        };
        for raw in raw_http_observations(&content) {
            self.observe_set_cookie(raw);
            if let Some(token) = extract_csrf_token(raw) {
                self.csrf_by_path.insert(path.clone(), token);
            }
        }
    }

    pub(super) fn inject(&self, call: &mut ModelToolCall) -> bool {
        if self.cookies.is_empty() && self.csrf_by_path.is_empty() {
            return false;
        }
        let Ok(mut arguments) = serde_json::from_str::<Value>(&call.arguments) else {
            return false;
        };
        let Some(object) = arguments.as_object_mut() else {
            return false;
        };
        let cookie = self.cookie_header();
        let mut changed = false;
        match call.name.as_str() {
            "http-framework-test" | "gobuster" | "sqlmap" | "sqlmap_scan" | "dalfox"
            | "web_xss_scan" | "file_upload_validate" => {
                changed |= merge_cookie_field(object, "cookies", &cookie);
            }
            "nuclei"
            | "web_vuln_scan"
            | "ffuf"
            | "command_injection_validate"
            | "csrf_validate"
            | "file_inclusion_validate"
            | "sqli_validate"
            | "xss_validate" => {
                changed |= merge_header_cookie_field(object, "header", &cookie);
            }
            "http_request" => {
                let headers = object
                    .entry("headers")
                    .or_insert_with(|| Value::Object(Default::default()));
                if let Some(headers) = headers.as_object_mut() {
                    if let Some(name) = headers
                        .keys()
                        .find(|name| name.eq_ignore_ascii_case("cookie"))
                        .cloned()
                    {
                        let explicit = headers.get(&name).and_then(Value::as_str).unwrap_or_default();
                        let merged = merge_cookie_headers(&cookie, explicit);
                        if merged != explicit {
                            headers.insert(name, Value::String(merged));
                            changed = true;
                        }
                    } else if !cookie.is_empty() {
                        headers.insert("Cookie".into(), Value::String(cookie.clone()));
                        changed = true;
                    }
                }
            }
            _ => {}
        }

        let form_post = call.name == "file_upload_validate"
            || object
                .get("method")
                .and_then(Value::as_str)
                .is_some_and(|method| method.eq_ignore_ascii_case("POST"));
        if matches!(
            call.name.as_str(),
            "http-framework-test" | "http_request" | "file_upload_validate"
        ) && form_post
            && let Some(path) = request_path(&Value::Object(object.clone()))
            && let Some((name, value)) = self.csrf_by_path.get(&path)
        {
            if let Some(form) = object.get_mut("form").and_then(Value::as_object_mut) {
                if !form.contains_key(name) {
                    form.insert(name.clone(), Value::String(value.clone()));
                    changed = true;
                }
            } else if call.name == "file_upload_validate" {
                object.insert(
                    "form".into(),
                    Value::Object(serde_json::Map::from_iter([(
                        name.clone(),
                        Value::String(value.clone()),
                    )])),
                );
                changed = true;
            } else {
                let body_key = if call.name == "http-framework-test" {
                    "data"
                } else {
                    "body"
                };
                if let Some(data) = object
                    .get(body_key)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    && !form_has_key(&data, name)
                {
                    object.insert(
                        body_key.into(),
                        Value::String(format!("{data}&{name}={}", form_encode(value))),
                    );
                    changed = true;
                }
            }
        }

        if changed {
            call.arguments = arguments.to_string();
        }
        changed
    }

    fn observe_request_auth(&mut self, arguments: &Value) {
        if let Some(cookies) = arguments.get("cookies").and_then(Value::as_str) {
            self.merge_cookie_header(cookies);
        }
        if let Some(headers) = arguments.get("headers").and_then(Value::as_object) {
            for (name, value) in headers {
                if name.eq_ignore_ascii_case("cookie")
                    && let Some(value) = value.as_str()
                {
                    self.merge_cookie_header(value);
                }
            }
        }
    }

    fn observe_set_cookie(&mut self, raw: &str) {
        for line in raw.lines() {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if !name.trim().eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            if let Some(pair) = value.trim().split(';').next() {
                self.merge_cookie_header(pair);
            }
        }
    }

    fn merge_cookie_header(&mut self, header: &str) {
        for pair in header.split(';') {
            let Some((name, value)) = pair.trim().split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() || matches!(name.to_ascii_lowercase().as_str(), "path" | "domain" | "expires" | "max-age" | "samesite") {
                continue;
            }
            self.cookies.insert(name.into(), value.trim().into());
        }
    }

    fn cookie_header(&self) -> String {
        self.cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

fn merge_cookie_field(
    object: &mut serde_json::Map<String, Value>,
    name: &str,
    session_cookie: &str,
) -> bool {
    if session_cookie.is_empty() {
        return false;
    }
    let explicit = object.get(name).and_then(Value::as_str).unwrap_or_default();
    let merged = merge_cookie_headers(session_cookie, explicit);
    if merged == explicit {
        return false;
    }
    object.insert(name.into(), Value::String(merged));
    true
}

fn merge_header_cookie_field(
    object: &mut serde_json::Map<String, Value>,
    name: &str,
    session_cookie: &str,
) -> bool {
    if session_cookie.is_empty() {
        return false;
    }
    let existing = object.get(name).and_then(Value::as_str).unwrap_or_default();
    let mut lines = existing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(index) = lines.iter().position(|line| {
        line.split_once(':')
            .is_some_and(|(header, _)| header.trim().eq_ignore_ascii_case("cookie"))
    }) {
        let explicit = lines[index]
            .split_once(':')
            .map(|(_, value)| value.trim())
            .unwrap_or_default();
        lines[index] = format!("Cookie: {}", merge_cookie_headers(session_cookie, explicit));
    } else {
        lines.push(format!("Cookie: {session_cookie}"));
    }
    let merged = lines.join("\n");
    if merged == existing {
        return false;
    }
    object.insert(name.into(), Value::String(merged));
    true
}

/// Merge cookies by name. Run-authenticated state supplies the baseline while
/// an explicit tool argument may override a single cookie without discarding
/// PHPSESSID/security and the rest of the authenticated session.
fn merge_cookie_headers(session_cookie: &str, explicit_cookie: &str) -> String {
    let mut cookies = BTreeMap::new();
    for header in [session_cookie, explicit_cookie] {
        for pair in header.split(';') {
            let Some((name, value)) = pair.trim().split_once('=') else {
                continue;
            };
            if !name.trim().is_empty() {
                cookies.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
    }
    cookies
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn raw_http_observations(value: &Value) -> Vec<&str> {
    let mut output = Vec::new();
    for key in ["raw_response", "stdout"] {
        if let Some(raw) = value.get(key).and_then(Value::as_str) {
            output.push(raw);
        }
    }
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        for result in results {
            output.extend(raw_http_observations(result));
        }
    }
    output
}

fn request_path(arguments: &Value) -> Option<String> {
    let target = ["url", "target"]
        .iter()
        .find_map(|name| arguments.get(name).and_then(Value::as_str))?;
    reqwest::Url::parse(target)
        .ok()
        .map(|url| url.path().to_string())
        .or_else(|| Some(target.split('?').next().unwrap_or("/").to_string()))
}

fn extract_csrf_token(raw: &str) -> Option<(String, String)> {
    let pattern = Regex::new(
        r#"(?i)<input[^>]+name=[\"'](user_token|csrf(?:_token)?|_token)[\"'][^>]+value=[\"']([^\"']+)[\"']"#,
    )
    .ok()?;
    let captures = pattern.captures(raw)?;
    Some((captures.get(1)?.as_str().into(), captures.get(2)?.as_str().into()))
}

fn form_has_key(data: &str, key: &str) -> bool {
    data.split('&').any(|pair| pair.split('=').next() == Some(key))
}

fn form_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_platform_pent_api::ToolExecutionClass;

    fn call(name: &str, arguments: &str) -> ModelToolCall {
        ModelToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: arguments.into(),
            execution_class: ToolExecutionClass::ReadOnly,
        }
    }

    #[test]
    fn propagates_login_cookie_to_scanners() {
        let mut state = WebSessionState::default();
        state.observe(
            r#"{"url":"http://dvwa/login.php"}"#,
            &serde_json::json!({"stdout":"HTTP/1.1 302 Found\r\nSet-Cookie: PHPSESSID=abc; path=/\r\nSet-Cookie: security=low\r\n\r\n"}).to_string(),
        );
        let mut nuclei = call("nuclei", r#"{"target":"http://dvwa"}"#);
        assert!(state.inject(&mut nuclei));
        assert!(nuclei.arguments.contains("PHPSESSID=abc"));
        assert!(nuclei.arguments.contains("security=low"));
    }

    #[test]
    fn merges_explicit_scanner_cookie_with_authenticated_session() {
        let mut state = WebSessionState::default();
        state.observe(
            r#"{"url":"http://dvwa/login.php"}"#,
            &serde_json::json!({"stdout":"HTTP/1.1 302 Found\r\nSet-Cookie: PHPSESSID=abc; path=/\r\nSet-Cookie: security=low\r\n\r\n"}).to_string(),
        );
        let mut sqlmap = call(
            "sqlmap_scan",
            r#"{"url":"http://dvwa/vulnerabilities/sqli/?id=1","cookies":"dvwaSession=2"}"#,
        );
        assert!(state.inject(&mut sqlmap));
        assert!(sqlmap.arguments.contains("PHPSESSID=abc"));
        assert!(sqlmap.arguments.contains("security=low"));
        assert!(sqlmap.arguments.contains("dvwaSession=2"));
    }

    #[test]
    fn merges_existing_cookie_header_for_nuclei() {
        let mut state = WebSessionState::default();
        state.observe(
            r#"{"url":"http://dvwa/login.php"}"#,
            &serde_json::json!({"stdout":"Set-Cookie: PHPSESSID=abc; path=/\r\nSet-Cookie: security=low"}).to_string(),
        );
        let mut nuclei = call(
            "nuclei",
            r#"{"target":"http://dvwa","header":"X-Test: yes\nCookie: dvwaSession=2"}"#,
        );
        assert!(state.inject(&mut nuclei));
        assert!(nuclei.arguments.contains("PHPSESSID=abc"));
        assert!(nuclei.arguments.contains("security=low"));
        assert!(nuclei.arguments.contains("dvwaSession=2"));
        assert!(nuclei.arguments.contains("X-Test: yes"));
    }

    #[test]
    fn injects_path_scoped_csrf_token_into_post_body() {
        let mut state = WebSessionState::default();
        state.observe(
            r#"{"url":"http://dvwa/upload/"}"#,
            &serde_json::json!({"stdout":"<input type='hidden' name='user_token' value='token123'>"}).to_string(),
        );
        let mut request = call(
            "http-framework-test",
            r#"{"url":"http://dvwa/upload/","method":"POST","data":"a=1"}"#,
        );
        assert!(state.inject(&mut request));
        assert!(request.arguments.contains("user_token=token123"));
    }

    #[test]
    fn injects_path_scoped_csrf_token_into_http_request_form() {
        let mut state = WebSessionState::default();
        state.observe(
            r#"{"url":"http://dvwa/vulnerabilities/exec/"}"#,
            &serde_json::json!({"raw_response":"<input type='hidden' name='user_token' value='token456'>"}).to_string(),
        );
        let mut request = call(
            "http_request",
            r#"{"url":"http://dvwa/vulnerabilities/exec/","method":"POST","form":{"ip":"127.0.0.1;id","Submit":"Submit"}}"#,
        );
        assert!(state.inject(&mut request));
        let arguments: Value = serde_json::from_str(&request.arguments).unwrap();
        assert_eq!(arguments["form"]["user_token"], "token456");
    }

    #[test]
    fn injects_auth_and_csrf_into_file_upload_validator() {
        let mut state = WebSessionState::default();
        state.observe(
            r#"{"url":"http://dvwa/vulnerabilities/upload/","headers":{"Cookie":"PHPSESSID=abc; security=low"}}"#,
            &serde_json::json!({"stdout":"<input type='hidden' name='user_token' value='upload-token'>"}).to_string(),
        );
        let mut request = call(
            "file_upload_validate",
            r#"{"url":"http://dvwa/vulnerabilities/upload/","filename":"marker.txt"}"#,
        );
        assert!(state.inject(&mut request));
        let arguments: Value = serde_json::from_str(&request.arguments).unwrap();
        assert_eq!(arguments["form"]["user_token"], "upload-token");
        assert!(arguments["cookies"].as_str().unwrap().contains("PHPSESSID=abc"));
        assert!(arguments["cookies"].as_str().unwrap().contains("security=low"));
    }
}
