//! RMOS Attack Graph — attack chain visualization and tracking.
//!
//! Models the penetration test as a directed graph of:
//! - Assets (target hosts)
//! - Endpoints (URLs/paths)
//! - Parameters (query params, form fields)
//! - Vulnerabilities (confirmed findings)
//! - Credentials (discovered passwords/hashes)
//! - Privileges (access levels gained)
//!
//! Edges represent attack progression: leads_to, depends_on, exploits, escalates.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Types of nodes in the attack graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Asset,
    Endpoint,
    Parameter,
    Vulnerability,
    Credential,
    Privilege,
}

/// Types of edges between nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// A → B: discovering A led to finding B
    LeadsTo,
    /// B depends on A (can't exploit B without A)
    DependsOn,
    /// A was used to exploit B
    Exploits,
    /// A escalated privileges to B
    Escalates,
}

/// A node in the attack graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub severity: Option<String>,
    pub metadata: Value,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackEdge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
    pub description: String,
}

/// The attack graph — nodes + edges.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttackGraph {
    pub nodes: HashMap<String, AttackNode>,
    pub edges: Vec<AttackEdge>,
    next_id: u64,
}

impl AttackGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an asset (target host).
    pub fn add_asset(&mut self, host: &str) -> String {
        let id = format!("asset-{}", host.replace([':', '/', '.'], "-"));
        if !self.nodes.contains_key(&id) {
            self.nodes.insert(
                id.clone(),
                AttackNode {
                    id: id.clone(),
                    name: host.to_string(),
                    kind: NodeKind::Asset,
                    severity: None,
                    metadata: Value::Null,
                },
            );
        }
        id
    }

    /// Add an endpoint under an asset.
    pub fn add_endpoint(&mut self, path: &str, method: &str, parent_asset: &str) -> String {
        let id = format!("ep-{}", self.next_id);
        self.next_id += 1;
        let asset_id = self.add_asset(parent_asset);
        self.nodes.insert(
            id.clone(),
            AttackNode {
                id: id.clone(),
                name: format!("{method} {path}"),
                kind: NodeKind::Endpoint,
                severity: None,
                metadata: serde_json::json!({"method": method, "path": path}),
            },
        );
        self.edges.push(AttackEdge {
            source: asset_id,
            target: id.clone(),
            kind: EdgeKind::LeadsTo,
            description: format!("资产 {} 暴露端点 {path}", parent_asset),
        });
        id
    }

    /// Add a vulnerability found at an endpoint.
    pub fn add_vulnerability(
        &mut self,
        vuln_type: &str,
        severity: &str,
        endpoint_id: &str,
        evidence: &[&str],
    ) -> String {
        let id = format!("vuln-{}", self.next_id);
        self.next_id += 1;
        self.nodes.insert(
            id.clone(),
            AttackNode {
                id: id.clone(),
                name: vuln_type.to_string(),
                kind: NodeKind::Vulnerability,
                severity: Some(severity.to_string()),
                metadata: serde_json::json!({"evidence": evidence}),
            },
        );
        self.edges.push(AttackEdge {
            source: endpoint_id.to_string(),
            target: id.clone(),
            kind: EdgeKind::Exploits,
            description: format!("{vuln_type} ({severity})"),
        });
        id
    }

    /// Add a credential discovered through a vulnerability.
    pub fn add_credential(&mut self, username: &str, source_vuln: &str) -> String {
        let id = format!("cred-{}", self.next_id);
        self.next_id += 1;
        self.nodes.insert(
            id.clone(),
            AttackNode {
                id: id.clone(),
                name: format!("用户: {username}"),
                kind: NodeKind::Credential,
                severity: None,
                metadata: serde_json::json!({"username": username}),
            },
        );
        self.edges.push(AttackEdge {
            source: source_vuln.to_string(),
            target: id.clone(),
            kind: EdgeKind::LeadsTo,
            description: format!("通过漏洞获取凭证 {username}"),
        });
        id
    }

    /// Add a privilege level gained through credentials or exploits.
    pub fn add_privilege(&mut self, level: &str, source_id: &str) -> String {
        let id = format!("priv-{}", self.next_id);
        self.next_id += 1;
        self.nodes.insert(
            id.clone(),
            AttackNode {
                id: id.clone(),
                name: level.to_string(),
                kind: NodeKind::Privilege,
                severity: None,
                metadata: Value::Null,
            },
        );
        self.edges.push(AttackEdge {
            source: source_id.to_string(),
            target: id.clone(),
            kind: EdgeKind::Escalates,
            description: format!("权限提升至 {level}"),
        });
        id
    }

    /// Generate a text representation of the current attack chain for the model.
    pub fn to_chain_summary(&self) -> String {
        if self.edges.is_empty() {
            return String::new();
        }

        // Find root nodes (no incoming edges)
        let targets: std::collections::HashSet<&str> =
            self.edges.iter().map(|e| e.target.as_str()).collect();
        let roots: Vec<&AttackEdge> = self
            .edges
            .iter()
            .filter(|e| !targets.contains(e.source.as_str()))
            .collect();

        if roots.is_empty() {
            return String::new();
        }

        let mut lines = vec![];
        for edge in &roots {
            let source = self.nodes.get(&edge.source);
            let target = self.nodes.get(&edge.target);
            if let (Some(src), Some(tgt)) = (source, target) {
                let emoji = node_emoji(&tgt.kind);
                lines.push(format!(
                    "  {} → {} {} ({})",
                    src.name, emoji, tgt.name, edge.description
                ));
                // Follow the chain one level deep
                self.follow_chain(&tgt.id, &mut lines, "    ", 2);
            }
        }

        if lines.is_empty() {
            return String::new();
        }
        format!("\n## 攻击链\n\n{}", lines.join("\n"))
    }

    fn follow_chain(&self, node_id: &str, lines: &mut Vec<String>, indent: &str, depth: usize) {
        if depth == 0 {
            return;
        }
        for edge in &self.edges {
            if edge.source == node_id {
                if let Some(target) = self.nodes.get(&edge.target) {
                    let emoji = node_emoji(&target.kind);
                    lines.push(format!(
                        "{indent}└─ {} {} ({})",
                        emoji, target.name, edge.description
                    ));
                    self.follow_chain(&target.id, lines, &format!("{indent}  "), depth - 1);
                }
            }
        }
    }

    /// Convert to the JSON format the frontend attack-graph endpoint expects.
    pub fn to_frontend_json(&self) -> Value {
        let nodes: Vec<Value> = self
            .nodes
            .values()
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "category": node_kind_category(&n.kind),
                    "symbolSize": node_kind_size(&n.kind, n.severity.as_deref()),
                    "severity": n.severity,
                })
            })
            .collect();

        let edges: Vec<Value> = self
            .edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "source": e.source,
                    "target": e.target,
                    "label": e.description,
                    "kind": e.kind,
                })
            })
            .collect();

        serde_json::json!({"nodes": nodes, "edges": edges})
    }
}

fn node_emoji(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Asset => "🖥",
        NodeKind::Endpoint => "🔗",
        NodeKind::Parameter => "📝",
        NodeKind::Vulnerability => "💥",
        NodeKind::Credential => "🔑",
        NodeKind::Privilege => "👑",
    }
}

fn node_kind_category(kind: &NodeKind) -> u8 {
    match kind {
        NodeKind::Asset => 0,
        NodeKind::Endpoint => 1,
        NodeKind::Parameter => 2,
        NodeKind::Vulnerability => 3,
        NodeKind::Credential => 4,
        NodeKind::Privilege => 5,
    }
}

fn node_kind_size(kind: &NodeKind, severity: Option<&str>) -> u8 {
    match kind {
        NodeKind::Vulnerability => match severity {
            Some("critical") => 40,
            Some("high") => 32,
            Some("medium") => 26,
            _ => 20,
        },
        NodeKind::Asset => 50,
        NodeKind::Privilege => 36,
        _ => 22,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_attack_chain() {
        let mut g = AttackGraph::new();
        let ep = g.add_endpoint("/login.php", "POST", "dvwa.local");
        let vuln = g.add_vulnerability("SQL Injection", "high", &ep, &["sqlmap-output"]);
        let cred = g.add_credential("admin", &vuln);
        g.add_privilege("admin panel access", &cred);

        let summary = g.to_chain_summary();
        assert!(summary.contains("SQL Injection"));
        assert!(summary.contains("admin"));
    }

    #[test]
    fn deduplicates_assets() {
        let mut g = AttackGraph::new();
        let a1 = g.add_asset("dvwa.local");
        let a2 = g.add_asset("dvwa.local");
        assert_eq!(a1, a2);
        assert_eq!(g.nodes.len(), 1);
    }
}
