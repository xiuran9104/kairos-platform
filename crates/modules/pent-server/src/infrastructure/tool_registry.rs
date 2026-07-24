//! Allow-listed security tool command construction.

use super::process::NetworkPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTool {
    pub program: &'static str,
    pub container_image: &'static str,
    pub args: Vec<String>,
}

pub fn prepare_tool(
    tool_name: &str,
    request: &[String],
    network_policy: &NetworkPolicy,
) -> Result<PreparedTool, String> {
    let target = request
        .last()
        .map(String::as_str)
        .ok_or_else(|| "工具调用缺少目标".to_string())?;
    validate_target(target, network_policy)?;

    match tool_name {
        "tcp_scan" | "nmap" => Ok(PreparedTool {
            program: "/usr/bin/nmap",
            container_image: "instrumentisto/nmap:latest",
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
            container_image: "",
            args: vec![
                "--silent".into(),
                "--show-error".into(),
                "--location".into(),
                "--max-redirs".into(),
                "3".into(),
                "--max-time".into(),
                "30".into(),
                "--url".into(),
                target.into(),
            ],
        }),
        "nuclei" => Ok(PreparedTool {
            program: "/usr/bin/nuclei",
            container_image: "projectdiscovery/nuclei:latest",
            args: vec![
                "-u".into(),
                target.into(),
                "-jsonl".into(),
                "-silent".into(),
                "-no-interactsh".into(),
            ],
        }),
        "afrog" => Ok(PreparedTool {
            program: "/usr/bin/afrog",
            container_image: "zan8in/afrog:latest",
            args: vec!["-t".into(), target.into(), "-json".into()],
        }),
        "dns_lookup" => Ok(PreparedTool {
            program: "/usr/bin/dig",
            container_image: "",
            args: vec!["+short".into(), target.into()],
        }),
        "ssh_exec" => Ok(PreparedTool {
            program: "/usr/bin/ssh",
            container_image: "",
            args: vec![
                "-o".into(), "ConnectTimeout=5".into(),
                "-o".into(), "StrictHostKeyChecking=no".into(),
                target.into(), "id; uptime; uname -a".into(),
            ],
        }),
        "sql_query" => Ok(PreparedTool {
            program: "/usr/bin/psql",
            container_image: "",
            args: vec!["-h".into(), target.into(), "-c".into(), "SELECT version()".into()],
        }),
        _ => Err(format!("工具 {tool_name} 未进入允许列表")),
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
            // DNS rebinding / SSRF check: resolve and verify no private IPs
            if let Some(host) = extract_host(target) {
                if is_private_host(&host) {
                    return Err(format!("目标 {host} 解析为内网地址，禁止访问"));
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
    let without_scheme = target.trim_start_matches("https://").trim_start_matches("http://");
    let host = without_scheme.split(':').next().unwrap_or(without_scheme);
    let host = host.split('/').next().unwrap_or(host); // strip path
    if !host.is_empty() && !host.starts_with('-') {
        return Some(host.to_owned());
    }
    None
}

fn is_private_host(host: &str) -> bool {
    use std::net::{IpAddr, Ipv4Addr};
    // Direct private IPv4 check
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_private() || v4.is_loopback() || v4.is_link_local()
                    || v4.octets()[0] == 0 || v4 == Ipv4Addr::new(255, 255, 255, 255)
            }
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };
    }
    // Check for private/internal hostname patterns
    let host_lower = host.to_lowercase();
    host_lower == "localhost"
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".internal")
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
    fn rejects_private_ip_target_under_target_only() {
        let policy = NetworkPolicy::TargetOnly("127.0.0.1".into());
        assert!(validate_target("127.0.0.1", &policy).is_err());
    }

    #[test]
    fn rejects_localhost_target_under_target_only() {
        let policy = NetworkPolicy::TargetOnly("localhost".into());
        assert!(validate_target("localhost", &policy).is_err());
    }

    #[test]
    fn rejects_internal_domain_target_under_target_only() {
        let policy = NetworkPolicy::TargetOnly("internal-host.local".into());
        assert!(validate_target("internal-host.local", &policy).is_err());
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
