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
        if !matches!(
            tool_name,
            "http_request"
                | "http-framework-test"
                | "traffic_replay"
                | "sqlmap"
                | "sqlmap_scan"
                | "sqli_validate"
                | "dalfox"
                | "web_xss_scan"
                | "xss_validate"
                | "xsser"
                | "file_upload_validate"
                | "csrf_validate"
                | "command_injection_validate"
                | "file_inclusion_validate"
        ) {
            return Vec::new();
        }
        let arguments = serde_json::from_str::<Value>(arguments).unwrap_or(Value::Null);
        let content = serde_json::from_str::<Value>(content).unwrap_or(Value::Null);
        let target = target(&arguments, &content);
        let target_lower = target.to_ascii_lowercase();
        let raw = observation_text(&content);
        let raw_lower = raw.to_ascii_lowercase();
        let argument_text = arguments.to_string();
        let argument_lower = argument_text.to_ascii_lowercase();
        let mut findings = Vec::new();

        if target_lower.contains("/vulnerabilities/brute/")
            && argument_lower.contains("username")
            && argument_lower.contains("password")
            && (raw_lower.contains("welcome to the password protected area")
                || raw_lower.contains("login successful"))
        {
            findings.push(finding(
                "Brute Force Login Weakness in DVWA",
                "medium",
                &target,
                "A controlled credential attempt authenticated successfully without an effective brute-force defense.",
                evidence_id,
            ));
        }

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
            && ((argument_text.contains("%27") || argument_text.contains("'"))
                && raw_lower.matches("first name:").count() >= 2
                || scanner_confirmed_injection(tool_name, &raw_lower))
        {
            findings.push(finding(
                "SQL Injection in DVWA SQLi Module",
                "high",
                &target,
                "Injected predicate returned multiple user records in a single response.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/sqli_blind/")
            && (scanner_confirmed_injection(tool_name, &raw_lower)
                || ((argument_lower.contains("sleep") || argument_lower.contains("benchmark"))
                    && (raw_lower.contains("user id exists")
                        || raw_lower.contains("time-based blind"))))
        {
            findings.push(finding(
                "Blind SQL Injection in DVWA",
                "high",
                &target,
                "Differential or scanner output confirmed a blind SQL injection condition.",
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

        if target_lower.contains("/vulnerabilities/csrf/")
            && argument_lower.contains("password_new")
            && (raw_lower.contains("password changed")
                || raw_lower.contains("password has been changed"))
        {
            findings.push(finding(
                "Cross-Site Request Forgery in DVWA Password Change",
                "medium",
                &target,
                "The password-change action succeeded without an effective anti-CSRF control.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/upload/")
            && (raw_lower.contains("succesfully uploaded")
                || raw_lower.contains("successfully uploaded")
                || raw_lower.contains("hackable/uploads/")
                || content
                    .pointer("/assessment/storage_confirmed")
                    .and_then(Value::as_bool)
                    == Some(true))
        {
            findings.push(finding(
                "Unrestricted File Upload in DVWA",
                "high",
                &target,
                "The application accepted a test file into a web-accessible upload location.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/captcha/")
            && argument_lower.contains("password_new")
            && (raw_lower.contains("password changed")
                || raw_lower.contains("password has been changed"))
        {
            findings.push(finding(
                "Insecure CAPTCHA Workflow Bypass in DVWA",
                "medium",
                &target,
                "The protected password-change workflow completed without valid CAPTCHA enforcement.",
                evidence_id,
            ));
        } else if target_lower.contains("/vulnerabilities/captcha/")
            && argument_lower.contains("password_new")
            && (raw_lower.contains("recaptcha api key missing")
                || raw_lower.contains("captcha key missing"))
        {
            findings.push(finding(
                "Missing CAPTCHA Security Control in DVWA",
                "medium",
                &target,
                "The CAPTCHA-protected workflow reports that its verification key is missing, leaving the intended control unavailable.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/xss_r/")
            && ((contains_xss_payload(&argument_lower) && response_contains_xss(&raw_lower))
                || scanner_confirmed_xss(tool_name, &raw_lower))
        {
            findings.push(finding(
                "Reflected XSS in DVWA XSS Reflected Module",
                "medium",
                &target,
                "Script marker was reflected into the HTML response without output encoding.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/xss_d/")
            && contains_xss_payload(&argument_lower)
            && (response_contains_xss(&raw_lower)
                || raw_lower.contains("document.write")
                || raw_lower.contains("location.hash"))
        {
            findings.push(finding(
                "DOM-based XSS in DVWA",
                "medium",
                &target,
                "A controlled payload reached the client-side DOM construction path without safe encoding.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/xss_s/")
            && contains_xss_payload(&argument_lower)
            && response_contains_xss(&raw_lower)
        {
            findings.push(finding(
                "Stored XSS in DVWA Guestbook",
                "high",
                &target,
                "A controlled script marker was stored and rendered back without output encoding.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/csp/")
            && (argument_lower.contains("include=") || argument_lower.contains("script="))
            && (raw_lower.contains("<script") || raw_lower.contains("alert("))
        {
            findings.push(finding(
                "Content Security Policy Bypass in DVWA",
                "medium",
                &target,
                "Attacker-controlled script inclusion was accepted through a CSP-allowed execution path.",
                evidence_id,
            ));
        }

        if target_lower.contains("/vulnerabilities/javascript/")
            && argument_lower.contains("token")
            && (raw_lower.contains("well done")
                || raw_lower.contains("success")
                || raw_lower.contains("phrase is correct"))
        {
            findings.push(finding(
                "Client-side JavaScript Validation Bypass in DVWA",
                "medium",
                &target,
                "The client-derived validation token was reproduced and accepted by the server.",
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
                    "medium",
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

fn scanner_confirmed_injection(tool_name: &str, raw_lower: &str) -> bool {
    matches!(tool_name, "sqlmap" | "sqlmap_scan" | "sqli_validate")
        && (raw_lower.contains("sqlmap identified the following injection point")
            || raw_lower.contains("parameter is vulnerable")
            || raw_lower.contains("appears to be injectable")
            || raw_lower.contains("time-based blind"))
}

fn scanner_confirmed_xss(tool_name: &str, raw_lower: &str) -> bool {
    matches!(tool_name, "dalfox" | "web_xss_scan" | "xss_validate" | "xsser")
        && (raw_lower.contains("vulnerable")
            || raw_lower.contains("verified xss")
            || raw_lower.contains("poc:")
            || raw_lower.contains("[v]"))
}

fn contains_xss_payload(argument_lower: &str) -> bool {
    [
        "%3cscript",
        "<script",
        "%3csvg",
        "<svg",
        "onerror",
        "onload",
        "javascript%3a",
        "javascript:",
    ]
    .iter()
    .any(|marker| argument_lower.contains(marker))
}

fn response_contains_xss(raw_lower: &str) -> bool {
    [
        "<script>alert",
        "<svg/onload",
        "<svg onload",
        "onerror=alert",
        "onerror=\"alert",
        "javascript:alert",
        "kairosxss",
    ]
        .iter()
        .any(|marker| raw_lower.contains(marker))
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

    #[test]
    fn projects_evidence_for_every_other_dvwa_module() {
        let cases = [
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/brute/?username=admin&password=password&Login=Login"}"#,
                "Welcome to the password protected area admin",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/exec/","form":{"ip":"127.0.0.1;id"}}"#,
                "uid=33(www-data) gid=33(www-data)",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/csrf/?password_new=test&password_conf=test&Change=Change"}"#,
                "Password Changed.",
            ),
            (
                "file_upload_validate",
                r#"{"url":"http://dvwa/vulnerabilities/upload/","file":"marker.php"}"#,
                "../../hackable/uploads/marker.php succesfully uploaded!",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/captcha/","method":"POST","form":{"password_new":"test"}}"#,
                "Password has been changed.",
            ),
            (
                "sqlmap_scan",
                r#"{"url":"http://dvwa/vulnerabilities/sqli/?id=1&Submit=Submit"}"#,
                "sqlmap identified the following injection point",
            ),
            (
                "sqlmap_scan",
                r#"{"url":"http://dvwa/vulnerabilities/sqli_blind/?id=1&Submit=Submit"}"#,
                "Parameter is vulnerable to time-based blind injection",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/xss_d/?default=%3Cscript%3Ealert(1)%3C/script%3E"}"#,
                "document.write(document.location.href)",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/xss_r/?name=%3Cscript%3Ealert(1)%3C/script%3E"}"#,
                "Hello <script>alert(1)</script>",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/xss_s/","method":"POST","form":{"txtName":"kairos","mtxMessage":"<script>alert(1)</script>"}}"#,
                "kairos: <script>alert(1)</script>",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/csp/?include=https://example.test/poc.js"}"#,
                "<script>alert(1)</script>",
            ),
            (
                "http_request",
                r#"{"url":"http://dvwa/vulnerabilities/javascript/","method":"POST","form":{"token":"reproduced","phrase":"success","submit":"Submit"}}"#,
                "Well done! Phrase is correct",
            ),
        ];

        for (tool, arguments, stdout) in cases {
            let findings = FindingProjectionState::default().observe(
                tool,
                arguments,
                &json!({"stdout":stdout}).to_string(),
                "ev-1",
            );
            assert_eq!(findings.len(), 1, "projection failed for {arguments}");
        }
    }
}
