//! Parsers for well-known security-tool output formats.

use kairos_platform_pent_api::{NormalizedFinding, ToolExecutionResult, ToolObservation};
use regex::Regex;
use serde_json::{Value, json};

use super::normalizer::normalize_tool_output;

pub fn normalize_security_tool_output(
    tool_name: &str,
    target: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    duration_ms: u64,
) -> ToolExecutionResult {
    let mut result =
        normalize_tool_output(tool_name, target, exit_code, stdout, stderr, duration_ms);
    let sanitized_output = result
        .observations
        .iter()
        .find(|observation| observation.key == "sanitized_text")
        .and_then(|observation| observation.value.as_str())
        .unwrap_or_default()
        .to_string();

    match tool_name.to_ascii_lowercase().as_str() {
        "nmap" | "tcp_scan" => parse_nmap(&sanitized_output, &mut result),
        "nuclei" => parse_json_lines(&sanitized_output, target, "nuclei", &mut result),
        "sqli_validate"
        | "xss_validate"
        | "csrf_validate"
        | "command_injection_validate"
        | "file_inclusion_validate"
        | "file_upload_validate" => {
            parse_json_lines(&sanitized_output, target, tool_name, &mut result)
        }
        "afrog" => parse_json_lines(&sanitized_output, target, "afrog", &mut result),
        "ffuf" => parse_ffuf(&sanitized_output, &mut result),
        _ => {}
    }
    result
}

fn parse_nmap(output: &str, result: &mut ToolExecutionResult) {
    let port_line =
        Regex::new(r"(?m)^\s*(\d{1,5})/(tcp|udp)\s+(open|filtered)\s+([^\s]+)(?:\s+(.+))?$")
            .expect("valid nmap regex");
    let mut ports = Vec::new();

    for captures in port_line.captures_iter(output) {
        let port = captures[1].parse::<u16>().unwrap_or_default();
        let protocol = captures[2].to_string();
        let state = captures[3].to_string();
        let service = captures[4].to_string();
        let product = captures
            .get(5)
            .map(|value| value.as_str().trim())
            .unwrap_or("");
        ports.push(json!({
            "port": port,
            "protocol": protocol,
            "state": state,
            "service": service,
            "product": product,
        }));
    }
    if !ports.is_empty() {
        result.summary = format!("发现 {} 个开放或受过滤端口", ports.len());
        result.observations.push(ToolObservation {
            category: "network".into(),
            key: "ports".into(),
            value: json!(ports),
            confidence: 0.95,
        });
    }
}

fn parse_json_lines(
    output: &str,
    fallback_target: &str,
    source: &str,
    result: &mut ToolExecutionResult,
) {
    for value in output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let info = value.get("info").unwrap_or(&Value::Null);
        let title = string_at(info, &["name"])
            .or_else(|| string_at(&value, &["name", "template-id", "template_id"]))
            .unwrap_or_else(|| format!("{source} 检测命中"));
        let severity = string_at(info, &["severity"]).unwrap_or_else(|| "info".to_string());
        let finding_target = string_at(&value, &["matched-at", "matched_at", "url", "host"])
            .unwrap_or_else(|| fallback_target.to_string());
        let description = string_at(info, &["description"])
            .unwrap_or_else(|| format!("{source} 规则命中：{title}"));

        result.findings.push(NormalizedFinding {
            title,
            severity,
            description,
            target: finding_target,
            confidence: 0.9,
            evidence_refs: vec![],
        });
    }
    if !result.findings.is_empty() {
        result.summary = format!("{source} 解析出 {} 条发现", result.findings.len());
    }
}

fn parse_ffuf(output: &str, result: &mut ToolExecutionResult) {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return;
    };
    let endpoints: Vec<Value> = value
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            json!({
                "url": item.get("url"),
                "status": item.get("status"),
                "length": item.get("length"),
                "content_type": item.get("content-type").or_else(|| item.get("content_type")),
            })
        })
        .collect();
    if !endpoints.is_empty() {
        result.summary = format!("FFUF 发现 {} 个端点", endpoints.len());
        result.observations.push(ToolObservation {
            category: "http".into(),
            key: "endpoints".into(),
            value: json!(endpoints),
            confidence: 0.95,
        });
    }
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nmap_ports() {
        let output =
            "PORT STATE SERVICE VERSION\n22/tcp open ssh OpenSSH 9.0\n80/tcp open http nginx";
        let result = normalize_security_tool_output("nmap", "host", 0, output, "", 10);
        assert_eq!(result.observations[1].key, "ports");
        assert_eq!(result.observations[1].value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn parses_nuclei_json_lines() {
        let output = r#"{"template-id":"cve-test","info":{"name":"测试漏洞","severity":"high","description":"描述"},"matched-at":"https://example.test"}"#;
        let result = normalize_security_tool_output("nuclei", "example.test", 0, output, "", 10);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, "high");
    }

    #[test]
    fn parses_ffuf_document() {
        let output =
            r#"{"results":[{"url":"https://example.test/admin","status":200,"length":42}]}"#;
        let result = normalize_security_tool_output("ffuf", "example.test", 0, output, "", 10);
        assert_eq!(result.observations[1].key, "endpoints");
    }
}
