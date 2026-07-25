//! Allow-listed security tool command construction.

use super::process::NetworkPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTool {
    pub program: &'static str,
    pub container_image: String,
    pub args: Vec<String>,
}

pub fn prepare_tool(
    tool_name: &str,
    request: &[String],
    network_policy: &NetworkPolicy,
) -> Result<PreparedTool, String> {
    let target = if tool_name == "http_request" {
        request.first()
    } else {
        request.last()
    }
    .map(String::as_str)
    .ok_or_else(|| "工具调用缺少目标".to_string())?;
    validate_target(target, network_policy)?;

    match tool_name {
        "tcp_scan" | "nmap" => Ok(PreparedTool {
            program: "/usr/bin/nmap",
            container_image: tool_image("TCP_SCAN", "instrumentisto/nmap:7.95"),
            args: vec![
                "-sT".into(),
                "-Pn".into(),
                "--reason".into(),
                "--".into(),
                target.into(),
            ],
        }),
        "http_probe" => Ok(PreparedTool {
            program: "/usr/bin/curl",
            container_image: tool_image("HTTP_PROBE", "curlimages/curl:8.12.1"),
            args: vec![
                "--silent".into(),
                "--show-error".into(),
                "--insecure".into(),
                "--dump-header".into(),
                "-".into(),
                "--max-redirs".into(),
                "0".into(),
                "--max-time".into(),
                "30".into(),
                "--url".into(),
                normalize_http_probe_target(target),
            ],
        }),
        "http_request" => {
            let url = request.first().ok_or("http_request 缺少 URL")?;
            let method = request.get(1).map(String::as_str).unwrap_or("GET");
            if !matches!(
                method,
                "GET" | "POST" | "PUT" | "DELETE" | "HEAD" | "OPTIONS" | "PATCH"
            ) {
                return Err(format!("不支持的 HTTP 方法: {method}"));
            }
            validate_target(url, network_policy)?;
            let mut args = vec![
                "--silent".into(),
                "--show-error".into(),
                "--insecure".into(),
                "--dump-header".into(),
                "-".into(),
                "--location".into(),
                "--max-redirs".into(),
                "5".into(),
                "--max-time".into(),
                "30".into(),
                "--cookie".into(),
                "/state/cookies.txt".into(),
                "--cookie-jar".into(),
                "/state/cookies.txt".into(),
                "--request".into(),
                method.into(),
            ];
            if let Some(headers) = request.get(3) {
                let headers: serde_json::Value =
                    serde_json::from_str(headers).map_err(|_| "HTTP headers 必须是 JSON 对象")?;
                let headers = headers.as_object().ok_or("HTTP headers 必须是 JSON 对象")?;
                for (name, value) in headers {
                    let value = value.as_str().ok_or("HTTP header 值必须是字符串")?;
                    if name.contains(['\r', '\n']) || value.contains(['\r', '\n']) {
                        return Err("HTTP header 不得包含换行符".into());
                    }
                    args.extend(["--header".into(), format!("{name}: {value}")]);
                }
            }
            if let Some(form) = request.get(4).filter(|value| !value.is_empty()) {
                let form: serde_json::Value =
                    serde_json::from_str(form).map_err(|_| "form 必须是 JSON 对象")?;
                let form = form.as_object().ok_or("form 必须是 JSON 对象")?;
                for (name, value) in form {
                    let value = value
                        .as_str()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| value.to_string());
                    args.extend(["--data-urlencode".into(), format!("{name}={value}")]);
                }
            } else if let Some(body) = request.get(2).filter(|value| !value.is_empty()) {
                args.extend(["--data-raw".into(), body.clone()]);
            }
            args.extend(["--url".into(), url.clone()]);
            Ok(PreparedTool {
                program: "/usr/bin/curl",
                container_image: tool_image("HTTP_REQUEST", "curlimages/curl:8.12.1"),
                args,
            })
        }
        "nuclei" => Ok(PreparedTool {
            program: "/usr/bin/nuclei",
            container_image: tool_image("NUCLEI", "projectdiscovery/nuclei:v3.3.10"),
            args: vec![
                "-u".into(),
                target.into(),
                "-jsonl".into(),
                "-silent".into(),
                "-no-interactsh".into(),
            ],
        }),
        "sqli_validate"
        | "xss_validate"
        | "csrf_validate"
        | "command_injection_validate"
        | "file_inclusion_validate"
        | "file_upload_validate" => {
            let tag = match tool_name {
                "sqli_validate" => "sqli",
                "xss_validate" => "xss",
                "csrf_validate" => "csrf",
                "command_injection_validate" => "cmdi",
                "file_inclusion_validate" => "lfi",
                "file_upload_validate" => "file-upload",
                _ => unreachable!(),
            };
            Ok(PreparedTool {
                program: "/usr/bin/nuclei",
                container_image: tool_image("NUCLEI", "projectdiscovery/nuclei:v3.3.10"),
                args: vec![
                    "-u".into(),
                    target.into(),
                    "-tags".into(),
                    tag.into(),
                    "-jsonl".into(),
                    "-silent".into(),
                    "-rate-limit".into(),
                    "5".into(),
                    "-bulk-size".into(),
                    "1".into(),
                    "-no-interactsh".into(),
                ],
            })
        }
        "afrog" => Ok(PreparedTool {
            program: "/usr/bin/afrog",
            container_image: tool_image("AFROG", "zan8in/afrog:v3.1.8"),
            args: vec!["-t".into(), target.into(), "-json".into()],
        }),
        "dns_lookup" => Ok(PreparedTool {
            program: "/usr/bin/dig",
            container_image: tool_image("DNS_LOOKUP", "nicolaka/netshoot:v0.13"),
            args: vec!["+short".into(), target.into()],
        }),
        "ssh_exec" => Ok(PreparedTool {
            program: "/usr/bin/ssh",
            container_image: tool_image("SSH_EXEC", "nicolaka/netshoot:v0.13"),
            args: vec![
                "-o".into(),
                "ConnectTimeout=5".into(),
                "-o".into(),
                "StrictHostKeyChecking=no".into(),
                target.into(),
                "id; uptime; uname -a".into(),
            ],
        }),
        "sql_query" => Ok(PreparedTool {
            program: "/usr/bin/psql",
            container_image: tool_image("SQL_QUERY", "postgres:17-alpine"),
            args: vec![
                "-h".into(),
                target.into(),
                "-c".into(),
                "SELECT version()".into(),
            ],
        }),
        _ => Err(format!("工具 {tool_name} 未进入允许列表")),
    }
}

fn tool_image(tool: &str, default: &str) -> String {
    std::env::var(format!("PENTA_TOOL_IMAGE_{tool}")).unwrap_or_else(|_| default.to_string())
}

fn normalize_http_probe_target(target: &str) -> String {
    let target = target.trim().trim_end_matches('/');
    if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("https://{target}")
    }
}

fn validate_target(target: &str, policy: &NetworkPolicy) -> Result<(), String> {
    if target.is_empty()
        || target.starts_with('-')
        || target.chars().any(char::is_whitespace)
        || target.contains('\0')
    {
        return Err("目标格式不安全".into());
    }
    match policy {
        NetworkPolicy::None => Err("当前策略禁止网络访问".into()),
        NetworkPolicy::TargetOnly(allowed) => {
            if !target_matches_scope(target, allowed) {
                return Err(format!("目标 {target} 不在本次授权范围"));
            }
            if let Some(host) = extract_host(target) {
                if is_forbidden_infrastructure_host(&host) {
                    return Err(format!("目标 {host} 属于平台保留地址，禁止访问"));
                }
            }
            Ok(())
        }
        NetworkPolicy::Full => Ok(()),
    }
}

fn extract_host(target: &str) -> Option<String> {
    // Try URL parse with scheme; fall back to bare host or host:port
    if let Ok(url) = reqwest::Url::parse(target) {
        if url.has_host() {
            return url.host_str().map(ToOwned::to_owned);
        }
        // URL parsed but no host (e.g. "example.test:8443" parsed as scheme:path)
        // Fall through to manual extraction
    }
    // Manual extraction: strip scheme if present, then split host from port
    let without_scheme = target
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = without_scheme.split(':').next().unwrap_or(without_scheme);
    let host = host.split('/').next().unwrap_or(host); // strip path
    if !host.is_empty() && !host.starts_with('-') {
        return Some(host.to_owned());
    }
    None
}

fn is_forbidden_infrastructure_host(host: &str) -> bool {
    use std::net::{IpAddr, Ipv4Addr};
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_multicast()
                    || v4 == Ipv4Addr::new(255, 255, 255, 255)
            }
            IpAddr::V6(v6) => v6.is_unspecified() || v6.is_multicast(),
        };
    }
    false
}

fn target_matches_scope(target: &str, allowed: &str) -> bool {
    if target == allowed {
        return true;
    }
    let target_host = extract_host(target);
    let allowed_host = extract_host(allowed);
    match (target_host, allowed_host) {
        (Some(t), Some(a)) => t == a,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_command_to_authorized_target() {
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        let prepared =
            prepare_tool("tcp_scan", &["scan".into(), "example.test".into()], &policy).unwrap();
        assert_eq!(prepared.program, "/usr/bin/nmap");
        assert_eq!(prepared.args.last().unwrap(), "example.test");
    }

    #[test]
    fn rejects_scope_escape_and_unknown_tools() {
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        assert!(prepare_tool("tcp_scan", &["other.test".into()], &policy).is_err());
        assert!(prepare_tool("shell", &["example.test".into()], &policy).is_err());
    }

    #[test]
    fn rejects_option_injection() {
        let policy = NetworkPolicy::Full;
        assert!(prepare_tool("tcp_scan", &["--script=unsafe".into()], &policy).is_err());
    }

    #[test]
    fn permits_url_derived_from_authorized_host() {
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        assert!(prepare_tool("http_probe", &["https://example.test".into()], &policy).is_ok());
        assert!(prepare_tool("http_probe", &["https://other.test".into()], &policy).is_err());
    }

    // ── DNS rebinding / SSRF tests ──────────────────────────────────────────

    #[test]
    fn allows_explicitly_authorized_private_ip_target() {
        let policy = NetworkPolicy::TargetOnly("127.0.0.1".into());
        assert!(validate_target("127.0.0.1", &policy).is_ok());
    }

    #[test]
    fn allows_explicitly_authorized_localhost_target() {
        let policy = NetworkPolicy::TargetOnly("localhost".into());
        assert!(validate_target("localhost", &policy).is_ok());
    }

    #[test]
    fn allows_explicitly_authorized_internal_domain_target() {
        let policy = NetworkPolicy::TargetOnly("internal-host.local".into());
        assert!(validate_target("internal-host.local", &policy).is_ok());
    }

    #[test]
    fn http_probe_does_not_duplicate_an_explicit_scheme() {
        assert_eq!(
            normalize_http_probe_target("https://example.test/"),
            "https://example.test"
        );
        assert_eq!(
            normalize_http_probe_target("example.test"),
            "https://example.test"
        );
    }

    #[test]
    fn allows_public_ip_under_target_only() {
        let policy = NetworkPolicy::TargetOnly("93.184.216.34".into()); // example.com
        assert!(validate_target("93.184.216.34", &policy).is_ok());
    }

    #[test]
    fn rejects_link_local_ip_under_target_only() {
        let policy = NetworkPolicy::TargetOnly("169.254.1.1".into());
        assert!(validate_target("169.254.1.1", &policy).is_err());
    }

    #[test]
    fn rejects_broadcast_ip_under_target_only() {
        let policy = NetworkPolicy::TargetOnly("255.255.255.255".into());
        assert!(validate_target("255.255.255.255", &policy).is_err());
    }

    #[test]
    fn validates_url_host_matches_allowed_scope() {
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        // URL parse extracts host, matches allowed scope
        assert!(validate_target("https://example.test/path", &policy).is_ok());
        assert!(validate_target("https://attacker.test", &policy).is_err());
    }

    #[test]
    fn validates_host_with_port_matches_scope() {
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        assert!(validate_target("example.test:8443", &policy).is_ok());
        assert!(validate_target("other.test:443", &policy).is_err());
    }

    // ── Option injection tests ──────────────────────────────────────────────

    #[test]
    fn rejects_empty_target() {
        assert!(validate_target("", &NetworkPolicy::Full).is_err());
        assert!(validate_target("  ", &NetworkPolicy::Full).is_err());
    }

    #[test]
    fn rejects_null_byte_injection() {
        assert!(validate_target("example.com\0.internal", &NetworkPolicy::Full).is_err());
    }

    #[test]
    fn rejects_newline_injection() {
        assert!(validate_target("example.com\n127.0.0.1", &NetworkPolicy::Full).is_err());
    }

    #[test]
    fn full_policy_allows_any_target() {
        assert!(validate_target("anything.example.com", &NetworkPolicy::Full).is_ok());
    }

    #[test]
    fn none_policy_rejects_all() {
        assert!(validate_target("example.com", &NetworkPolicy::None).is_err());
    }
}
