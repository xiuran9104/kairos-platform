use std::collections::HashSet;

use serde_json::{Value, json};

/// Deterministic projections for high-signal HTTP evidence. This complements
/// model extraction: a terminal summarization failure must not discard a
/// vulnerability whose proof is already present in governed Evidence.
#[derive(Default)]
pub(super) struct FindingProjectionState {
    weak_session_values: HashSet<String>,
}

impl FindingProjectionState {
    pub(super) fn observe(
        &mut self,
        tool_name: &str,
        arguments: &str,
        content: &str,
        evidence_id: &str,
    ) -> Vec<Value> {
        if !matches!(tool_name, "http_request" | "http-framework-test" | "traffic_replay") {
            return Vec::new();
        }
        let arguments = serde_json::from_str::<Value>(arguments).unwrap_or(Value::Null);
        let content = serde_json::from_str::<Value>(content).unwrap_or(Value::Null);
        let target = target(&arguments, &content);
        let target_lower = target.to_ascii_lowercase();
        let raw = observation_text(&content);
        let raw_lower = raw.to_ascii_lowercase();
        let argument_text = arguments.to_string();
        let mut findings = Vec::new();

        if target_lower.contains("/vulnerabilities/fi/")
            && (raw.contains("root:x:0:0:") || raw.contains("daemon:x:1:1:"))
        {
            findings.push(finding(
                "Local File Inclusion in DVWA File Inclusion Module",
                "high",
                &target,
                "Traversal payload returned operating-system account data including root:x:0:0.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/sqli/")
            && !target_lower.contains("sqli_blind")
            && (argument_text.contains("%27") || argument_text.contains("'"))
            && raw_lower.matches("first name:").count() >= 2
        {
            findings.push(finding(
                "SQL Injection in DVWA SQLi Module",
                "high",
                &target,
                "Injected predicate returned multiple user records in a single response.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/exec/")
            && (argument_text.contains(';') || argument_text.contains("%3b"))
            && (raw_lower.contains("uid=") || raw_lower.contains("gid="))
        {
            findings.push(finding(
                "Command Injection in DVWA Exec Module",
                "critical",
                &target,
                "Command separator payload returned operating-system uid/gid output.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/xss_r/")
            && (argument_text.to_ascii_lowercase().contains("%3cscript")
                || argument_text.to_ascii_lowercase().contains("<script"))
            && raw_lower.contains("<script")
        {
            findings.push(finding(
                "Reflected XSS in DVWA XSS Reflected Module",
                "medium",
                &target,
                "Script marker was reflected into the HTML response without output encoding.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/weak_id/") {
            for value in set_cookie_values(&raw, "dvwaSession") {
                self.weak_session_values.insert(value);
            }
            if self.weak_session_values.len() >= 2 {
                let mut values = self.weak_session_values.iter().cloned().collect::<Vec<_>>();
                values.sort();
                findings.push(finding(
                    "Predictable Weak Session IDs in DVWA",
                    "high",
                    &target,
                    &format!(
                        "Repeated session generation produced predictable distinct dvwaSession values: {}.",
                        values.join(", ")
                    ),
                    evidence_id,
                ));
            }
        }

        findings
    }
}

fn finding(
    title: &str,
    severity: &str,
    target: &str,
    description: &str,
    evidence_id: &str,
) -> Value {
    json!({
        "title": title,
        "severity": severity,
        "target": target,
        "description": description,
        "confidence": 1.0,
        "evidence_ids": [evidence_id],
    })
}

fn target(arguments: &Value, content: &Value) -> String {
    let target = ["url", "target"]
        .iter()
        .find_map(|name| arguments.get(name).and_then(Value::as_str))
        .or_else(|| content.get("url").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    if target.contains("/vulnerabilities/")
        && let Ok(mut url) = reqwest::Url::parse(&target)
    {
        url.set_query(None);
        url.set_fragment(None);
        return url.to_string();
    }
    target
}

fn observation_text(value: &Value) -> String {
    let mut parts = Vec::new();
    collect_observation_text(value, &mut parts);
    parts.join("\n")
}

fn collect_observation_text(value: &Value, output: &mut Vec<String>) {
    for key in ["stdout", "raw_response", "body"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            output.push(text.to_string());
        }
    }
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        for result in results {
            collect_observation_text(result, output);
        }
    }
}

fn set_cookie_values(raw: &str, cookie_name: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let (header, value) = line.split_once(':')?;
            if !header.trim().eq_ignore_ascii_case("set-cookie") {
                return None;
            }
            let (name, value) = value.trim().split(';').next()?.split_once('=')?;
            name.trim()
                .eq_ignore_ascii_case(cookie_name)
                .then(|| value.trim().to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_lfi_from_passwd_evidence() {
        let findings = FindingProjectionState::default().observe(
            "http-framework-test",
            r#"{"url":"http://dvwa/vulnerabilities/fi/?page=../../../../../etc/passwd"}"#,
            &json!({"stdout":"HTTP/1.1 200 OK\r\n\r\nroot:x:0:0:root:/root:/bin/bash"}).to_string(),
            "ev-1",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["severity"], "high");
        assert_eq!(findings[0]["evidence_ids"][0], "ev-1");
    }

    #[test]
    fn weak_session_requires_two_distinct_cookie_values() {
        let mut state = FindingProjectionState::default();
        let args = r#"{"url":"http://dvwa/vulnerabilities/weak_id/"}"#;
        assert!(state
            .observe(
                "http-framework-test",
                args,
                &json!({"stdout":"Set-Cookie: dvwaSession=1; path=/"}).to_string(),
                "ev-1"
            )
            .is_empty());
        assert_eq!(
            state
                .observe(
                    "http-framework-test",
                    args,
                    &json!({"stdout":"Set-Cookie: dvwaSession=2; path=/"}).to_string(),
                    "ev-2"
                )
                .len(),
            1
        );
    }
}
