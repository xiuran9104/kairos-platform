//! Converts heterogeneous tool output into the planning-layer contract.

use chrono::Utc;
use kairos_platform_pent_api::{
    ToolExecutionMetrics, ToolExecutionResult, ToolExecutionStatus, ToolObservation,
};
use regex::Regex;
use serde_json::json;

const MAX_TEXT_BYTES: usize = 16 * 1024;

pub fn normalize_tool_output(
    tool_name: &str,
    target: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    duration_ms: u64,
) -> ToolExecutionResult {
    let stdout = sanitize(stdout);
    let stderr = sanitize(stderr);
    let status = match (exit_code, stdout.is_empty()) {
        (0, _) => ToolExecutionStatus::Succeeded,
        (_, false) => ToolExecutionStatus::Partial,
        _ => ToolExecutionStatus::Failed,
    };
    let summary = if stdout.is_empty() {
        if stderr.is_empty() {
            format!("{tool_name} 未返回输出")
        } else {
            first_line(&stderr)
        }
    } else {
        first_line(&stdout)
    };
    let observations = (!stdout.is_empty())
        .then(|| ToolObservation {
            category: "tool_output".into(),
            key: "sanitized_text".into(),
            value: json!(stdout),
            confidence: 0.5,
        })
        .into_iter()
        .collect();

    ToolExecutionResult {
        schema_version: 1,
        tool_name: tool_name.into(),
        target: target.into(),
        status,
        summary,
        observations,
        findings: vec![],
        artifacts: vec![],
        raw_output_ref: None,
        metrics: ToolExecutionMetrics {
            exit_code,
            duration_ms,
        },
        warnings: (!stderr.is_empty()).then_some(stderr).into_iter().collect(),
        occurred_at: Utc::now(),
    }
}

fn first_line(value: &str) -> String {
    value
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect()
}

fn sanitize(value: &str) -> String {
    let ansi = Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("valid ANSI regex");
    let secret = Regex::new(
        r#"(?i)\b(authorization|cookie|set-cookie|api[_-]?key|password|secret)\b(["']?\s*[:=]\s*["']?)([^"'\s,;}]+)"#,
    )
    .expect("valid secret regex");
    let cleaned: String = ansi
        .replace_all(value, "")
        .chars()
        .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
        .collect();
    let redacted = secret.replace_all(&cleaned, "$1$2[REDACTED]");
    truncate_utf8(redacted.trim(), MAX_TEXT_BYTES).to_string()
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_terminal_codes_and_redacts_secrets() {
        let result = normalize_tool_output(
            "http_probe",
            "example.test",
            0,
            "\u{1b}[31mHTTP 200\u{1b}[0m\nAuthorization: Bearer-token",
            "cookie=session-value",
            12,
        );
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("HTTP 200"));
        assert!(!serialized.contains("Bearer-token"));
        assert!(!serialized.contains("session-value"));
        assert_eq!(result.status, ToolExecutionStatus::Succeeded);
    }

    #[test]
    fn preserves_partial_output_on_failure() {
        let result = normalize_tool_output("nmap", "127.0.0.1", 2, "port 80 open", "timeout", 50);
        assert_eq!(result.status, ToolExecutionStatus::Partial);
        assert_eq!(result.metrics.exit_code, 2);
    }

    #[test]
    fn redacts_secrets_without_breaking_json() {
        let result = normalize_tool_output(
            "scanner",
            "host",
            0,
            r#"{"api_key":"private-value","status":"ok"}"#,
            "",
            1,
        );
        let value = result.observations[0].value.as_str().unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(value).is_ok());
        assert!(!value.contains("private-value"));
    }
}
