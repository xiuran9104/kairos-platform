//! RMOS Pentest Memory — structured penetration test state.
//!
//! Replaces the flat `run_summary` string with structured findings,
//! attack surface, next actions, and credentials tracking.
//! Injected into the conversation system prompt each turn so the
//! agent always knows "where we are and what's next".

use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::collections::HashSet;

/// Serialized pentest state persisted alongside the run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PentestMemory {
    /// Target under test.
    pub target: String,
    /// Technologies detected (Apache, PHP, MySQL, etc.).
    pub technologies: Vec<String>,
    /// Confirmed vulnerabilities.
    pub findings: Vec<Finding>,
    /// Discovered attack surface (endpoints, parameters, paths).
    pub attack_surface: Vec<AttackSurfaceEntry>,
    /// Recommended next actions, ordered by priority.
    pub next_actions: Vec<NextAction>,
    /// Credentials or secrets discovered during the test.
    pub credentials: Vec<DiscoveredCredential>,
    /// Track what knowledge has already been injected to avoid duplicates.
    pub knowledge_loaded: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    #[serde(rename = "type")]
    pub vuln_type: String,
    pub endpoint: String,
    pub parameter: Option<String>,
    pub severity: Severity,
    pub status: FindingStatus,
    pub description: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Suspected,
    Confirmed,
    FalsePositive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackSurfaceEntry {
    pub path: String,
    pub method: String,
    pub content_type: Option<String>,
    pub auth_required: bool,
    pub discovered_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextAction {
    pub priority: u8,
    pub action: String,
    pub tool: Option<String>,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredCredential {
    pub username: String,
    pub password_hash: Option<String>,
    pub source: String,
}

impl PentestMemory {
    pub fn new(target: &str) -> Self {
        Self {
            target: target.to_string(),
            ..Default::default()
        }
    }

    /// Update from a tool result's JSON content.
    /// Extracts technologies, endpoints, and findings heuristically.
    pub fn absorb_tool_result(&mut self, tool_name: &str, result: &Value) {
        let output = result
            .get("result")
            .or_else(|| result.get("output"))
            .unwrap_or(result);
        let output_str = output.to_string().to_ascii_lowercase();

        // Extract technologies from whatweb/nmap output
        self.extract_technologies(&output_str);

        // Extract endpoints from discovery tools
        if matches!(tool_name, "dir_fuzz" | "gobuster" | "ffuf" | "dirsearch") {
            self.extract_attack_surface(output, tool_name);
        }

        // Extract findings from vulnerability scanners
        if matches!(tool_name, "nuclei" | "dalfox" | "sqlmap" | "web_vuln_scan") {
            self.extract_findings(output, tool_name);
        }
    }

    fn extract_technologies(&mut self, output: &str) {
        let tech_signatures: &[(&str, &str)] = &[
            ("apache", "Apache"),
            ("nginx", "Nginx"),
            ("php/", "PHP"),
            ("mysql", "MySQL"),
            ("mariadb", "MariaDB"),
            ("postgresql", "PostgreSQL"),
            ("jquery", "jQuery"),
            ("wordpress", "WordPress"),
            ("django", "Django"),
            ("laravel", "Laravel"),
            ("tomcat", "Tomcat"),
            ("iis", "IIS"),
            ("node.js", "Node.js"),
            ("react", "React"),
            ("vue", "Vue.js"),
        ];
        for (sig, name) in tech_signatures {
            if output.contains(sig) && !self.technologies.contains(&name.to_string()) {
                self.technologies.push(name.to_string());
            }
        }
    }

    fn extract_attack_surface(&mut self, output: &Value, discovered_by: &str) {
        // Try common JSON fields for endpoint discovery
        let paths: Vec<&str> = output
            .get("results")
            .and_then(|v| v.as_array())
            .or_else(|| output.get("urls").and_then(|v| v.as_array()))
            .or_else(|| output.get("discoveries").and_then(|v| v.as_array()))
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        item.get("path")
                            .or_else(|| item.get("url"))
                            .and_then(|v| v.as_str())
                    })
                    .collect()
            })
            .unwrap_or_default();

        let existing: HashSet<String> =
            self.attack_surface.iter().map(|a| a.path.clone()).collect();

        for path in paths {
            if !existing.contains(path) && path.len() <= 200 {
                self.attack_surface.push(AttackSurfaceEntry {
                    path: path.to_string(),
                    method: "GET".to_string(),
                    content_type: None,
                    auth_required: false,
                    discovered_by: discovered_by.to_string(),
                });
            }
        }
    }

    fn extract_findings(&mut self, output: &Value, _tool_name: &str) {
        let findings: Vec<&Value> = output
            .get("findings")
            .and_then(|v| v.as_array())
            .or_else(|| output.get("results").and_then(|v| v.as_array()))
            .map(|arr| arr.iter().collect())
            .unwrap_or_default();

        for f in findings {
            let title = f
                .get("title")
                .or_else(|| f.get("name"))
                .or_else(|| f.get("info").and_then(|i| i.get("name")))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let severity = f
                .get("severity")
                .or_else(|| f.get("risk"))
                .or_else(|| f.get("info").and_then(|i| i.get("severity")))
                .and_then(|v| v.as_str())
                .map(|s| match s.to_ascii_lowercase().as_str() {
                    "critical" => Severity::Critical,
                    "high" => Severity::High,
                    "medium" => Severity::Medium,
                    "low" => Severity::Low,
                    _ => Severity::Info,
                })
                .unwrap_or(Severity::Info);

            let endpoint = f
                .get("target")
                .or_else(|| f.get("url"))
                .or_else(|| f.get("host"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let dup = self
                .findings
                .iter()
                .any(|existing| existing.endpoint == endpoint && existing.vuln_type == title);

            if !dup && !title.is_empty() {
                let finding = Finding {
                    id: format!("f-{}", self.findings.len() + 1),
                    vuln_type: title.to_string(),
                    endpoint: endpoint.to_string(),
                    parameter: f
                        .get("parameter")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    severity,
                    status: FindingStatus::Suspected,
                    description: f
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or(title)
                        .to_string(),
                    evidence_refs: vec![],
                };
                self.findings.push(finding);
            }
        }
    }

    /// Generate next_actions for methodology categories not yet covered.
    /// Called after each round to suggest what to test next.
    pub fn ensure_coverage_actions(&mut self) {
        let vuln_types: HashSet<&str> =
            self.findings.iter().map(|f| f.vuln_type.as_str()).collect();
        // M3: Dynamic vulnerability targets — use actually-discovered attack surface
        // paths instead of hardcoded Juice Shop endpoints.
        let discovered: Vec<String> = self.attack_surface.iter().map(|a| a.path.clone()).collect();
        let fallback_target = self.target.clone();

        let checklist: &[(&str, &str, u8)] = &[
            ("SQL Injection", "sqlmap", 1),
            ("NoSQL Injection", "http_request", 1),
            ("XSS Reflected", "dalfox", 1),
            ("XSS Stored", "dalfox", 1),
            ("XSS DOM", "dalfox", 1),
            ("IDOR", "http_request", 2),
            ("JWT Forgery", "http_request", 2),
            ("SSRF", "http_request", 2),
            ("Broken Access Control", "http_request", 2),
            ("Sensitive Data Exposure", "http_request", 2),
            ("Security Misconfiguration", "http_request", 2),
            ("File Upload", "file_upload_validate", 2),
            ("Path Traversal", "http_request", 2),
            ("CSRF", "http_request", 2),
            ("Weak Password", "http_request", 2),
        ];

        for (vuln, tool, priority) in checklist {
            if !vuln_types.contains(vuln) {
                let already_planned = self.next_actions.iter().any(|a| a.action.contains(vuln));
                if !already_planned {
                    let target = discovered
                        .first()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| fallback_target.clone());
                    self.next_actions.push(NextAction {
                        priority: *priority,
                        action: format!("检测 {vuln} 漏洞"),
                        tool: Some(tool.to_string()),
                        target,
                        reason: format!("{vuln} 尚未检测，方法论中标记为必测项"),
                    });
                }
            }
        }
    }

    /// Generate structured context for injection into the system prompt.
    /// Compact enough to fit within token budget.
    pub fn to_context_summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Target + technologies
        if !self.target.is_empty() {
            parts.push(format!("**目标**: {}", self.target));
        }
        if !self.technologies.is_empty() {
            parts.push(format!("**技术栈**: {}", self.technologies.join(", ")));
        }

        // Confirmed findings
        let confirmed: Vec<&Finding> = self
            .findings
            .iter()
            .filter(|f| f.status == FindingStatus::Confirmed)
            .collect();
        if !confirmed.is_empty() {
            let mut lines = vec!["**已确认漏洞**:".to_string()];
            for f in &confirmed {
                lines.push(format!(
                    "  - {} ({}) @ {}",
                    f.vuln_type,
                    severity_emoji(&f.severity),
                    f.endpoint
                ));
            }
            parts.push(lines.join("\n"));
        }

        // Suspected findings
        let suspected: Vec<&Finding> = self
            .findings
            .iter()
            .filter(|f| f.status == FindingStatus::Suspected)
            .collect();
        if !suspected.is_empty() {
            let mut lines = vec!["**待验证线索**:".to_string()];
            for f in &suspected {
                lines.push(format!(
                    "  - {} @ {} — {}",
                    f.vuln_type, f.endpoint, f.description
                ));
            }
            parts.push(lines.join("\n"));
        }

        // Next actions
        if !self.next_actions.is_empty() {
            let mut lines = vec!["**建议下一步**:".to_string()];
            let mut actions: Vec<&NextAction> = self.next_actions.iter().collect();
            actions.sort_by_key(|a| a.priority);
            for a in actions.iter().take(5) {
                let tool = a.tool.as_deref().unwrap_or("manual");
                lines.push(format!(
                    "  [P{}] {} — 用 `{}` 对 {} 执行：{}",
                    a.priority, a.reason, tool, a.target, a.action
                ));
            }
            parts.push(lines.join("\n"));
        }

        if parts.is_empty() {
            return String::new();
        }
        format!("\n## 渗透进度\n\n{}", parts.join("\n\n"))
    }
}

fn severity_emoji(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "🔴 CRITICAL",
        Severity::High => "🟠 HIGH",
        Severity::Medium => "🟡 MEDIUM",
        Severity::Low => "🔵 LOW",
        Severity::Info => "⚪ INFO",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_technologies_from_whatweb_output() {
        let mut m = PentestMemory::new("example.com");
        m.absorb_tool_result(
            "whatweb",
            &json!({
                "result": "Apache/2.4.41 PHP/7.4.3 MySQL"
            }),
        );
        assert!(m.technologies.contains(&"Apache".to_string()));
        assert!(m.technologies.contains(&"PHP".to_string()));
        assert!(m.technologies.contains(&"MySQL".to_string()));
    }

    #[test]
    fn deduplicates_findings() {
        let mut m = PentestMemory::new("example.com");
        m.absorb_tool_result(
            "nuclei",
            &json!({
                "findings": [
                    {"title": "SQL Injection", "severity": "high", "target": "/login.php"}
                ]
            }),
        );
        m.absorb_tool_result(
            "nuclei",
            &json!({
                "findings": [
                    {"title": "SQL Injection", "severity": "high", "target": "/login.php"}
                ]
            }),
        );
        assert_eq!(m.findings.len(), 1);
    }

    #[test]
    fn generates_empty_summary_for_default() {
        let m = PentestMemory::default();
        assert!(m.to_context_summary().is_empty());
    }

    #[test]
    fn summary_includes_findings_and_next_actions() {
        let mut m = PentestMemory::new("dvwa.local");
        m.technologies.push("PHP".into());
        m.findings.push(Finding {
            id: "f-1".into(),
            vuln_type: "SQL Injection".into(),
            endpoint: "/vulnerabilities/sqli/".into(),
            parameter: Some("id".into()),
            severity: Severity::High,
            status: FindingStatus::Confirmed,
            description: "Blind SQL injection".into(),
            evidence_refs: vec!["ev-1".into()],
        });
        m.next_actions.push(NextAction {
            priority: 1,
            action: "Dump database".into(),
            tool: Some("sqlmap".into()),
            target: "/vulnerabilities/sqli/?id=1".into(),
            reason: "SQL injection confirmed".into(),
        });

        let summary = m.to_context_summary();
        assert!(summary.contains("SQL Injection"));
        assert!(summary.contains("sqlmap"));
        assert!(summary.contains("dvwa.local"));
    }
}
