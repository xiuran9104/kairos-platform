use serde_json::Value;

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

pub(super) fn validate_tool_scope(
    tool_name: &str,
    arguments: &str,
    scope_snapshot: &Value,
) -> Result<(), String> {
    if !NETWORK_TOOLS.contains(&tool_name) {
        return Ok(());
    }
    if scope_snapshot.get("confirmed_at").is_none()
        && scope_snapshot.get("confirmed").and_then(Value::as_bool) != Some(true)
    {
        return Err("目标范围尚未人工确认，禁止执行网络工具".into());
    }

    let arguments: Value =
        serde_json::from_str(arguments).map_err(|error| format!("工具参数无效: {error}"))?;
    let requested_target = arguments
        .get("url")
        .or_else(|| arguments.get("target"))
        .or_else(|| arguments.get("domain"))
        .and_then(Value::as_str)
        .ok_or_else(|| "网络工具缺少 url/target/domain".to_string())?;
    let requested_host =
        target_host(requested_target).ok_or_else(|| "无法识别工具目标主机".to_string())?;
    let allowed = scope_snapshot
        .get("targets")
        .and_then(Value::as_array)
        .is_some_and(|targets| {
            targets.iter().filter_map(Value::as_str).any(|target| {
                target_host(target).is_some_and(|allowed_host| allowed_host == requested_host)
            })
        });
    if !allowed {
        return Err(format!("目标 {requested_host} 不在已确认授权范围"));
    }
    Ok(())
}

fn target_host(target: &str) -> Option<String> {
    let target = target.trim();
    if let Ok(url) = reqwest::Url::parse(target) {
        return url.host_str().map(|host| host.to_ascii_lowercase());
    }
    let without_scheme = target
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .split(':')
        .next()
        .unwrap_or(without_scheme)
        .trim();
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate_tool_scope;

    #[test]
    fn blocks_network_tools_before_scope_confirmation() {
        let scope = json!({"targets":["https://example.test"],"confirmed":false});
        let error =
            validate_tool_scope("http_request", r#"{"url":"https://example.test/"}"#, &scope)
                .unwrap_err();
        assert!(error.contains("尚未人工确认"));
    }

    #[test]
    fn allows_only_matching_confirmed_host() {
        let scope = json!({
            "targets":["https://example.test/app"],
            "confirmed_at":"2026-07-24T00:00:00Z"
        });
        assert!(
            validate_tool_scope(
                "http_request",
                r#"{"url":"https://example.test/api"}"#,
                &scope
            )
            .is_ok()
        );
        assert!(
            validate_tool_scope(
                "http_request",
                r#"{"url":"https://other.test/api"}"#,
                &scope
            )
            .is_err()
        );
    }

    #[test]
    fn upload_validator_uses_confirmed_session_authorization() {
        let scope = json!({
            "targets":["example.test"],
            "confirmed_at":"2026-07-24T00:00:00Z"
        });
        assert!(
            validate_tool_scope(
                "file_upload_validate",
                r#"{"url":"https://example.test/upload"}"#,
                &scope
            )
            .is_ok()
        );
    }
}
