//! RMOS Knowledge Cache — layered wiki search with deduplication.
//!
//! Wiki layers:
//! - technology    — 技术栈知识（Apache CVE, Struts 漏洞历史）
//! - vulnerability — 漏洞原理（SQL 注入分类, XSS 绕过技术）
//! - methodology   — 渗透流程（怎么做信息收集, 怎么验证漏洞）
//! - tool_usage    — 工具用法（sqlmap 参数, nuclei 模板）
//! - exploit_case  — 实战案例（DVWA SQL 注入解法）
//! - remediation   — 修复建议（给报告的）
//!
//! Only queries a layer when the triggering technology/vulnerability
//! hasn't been loaded before, avoiding redundant token consumption.

use kairos_platform_framework_database::PgPool;
use kairos_platform_pent_runtime::ports::RuntimeKnowledgeLayer as KnowledgeLayer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// Which wiki layer to search.
/// Cache entry tracking what knowledge has been loaded.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeCache {
    /// Technologies whose wiki has been loaded.
    pub technologies_loaded: HashSet<String>,
    /// Vulnerability types whose wiki has been loaded.
    pub vulnerabilities_loaded: HashSet<String>,
    /// Tool names whose usage has been loaded.
    pub tools_loaded: HashSet<String>,
    /// Whether general methodology has been injected.
    pub methodology_loaded: bool,
}

impl KnowledgeCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a technology triggers new knowledge search.
    pub fn needs_technology_search(&self, tech: &str) -> bool {
        !self.technologies_loaded.contains(tech)
    }

    /// Check if a vulnerability triggers new knowledge search.
    pub fn needs_vulnerability_search(&self, vuln_type: &str) -> bool {
        !self.vulnerabilities_loaded.contains(vuln_type)
    }

    /// Check if a tool's usage should be looked up.
    pub fn needs_tool_search(&self, tool: &str) -> bool {
        !self.tools_loaded.contains(tool)
    }

    /// Check if methodology should be injected (only once per session).
    pub fn needs_methodology(&self) -> bool {
        !self.methodology_loaded
    }

    /// Mark a technology as loaded.
    pub fn mark_technology(&mut self, tech: &str) {
        self.technologies_loaded.insert(tech.to_string());
    }

    /// Mark a vulnerability type as loaded.
    pub fn mark_vulnerability(&mut self, vuln: &str) {
        self.vulnerabilities_loaded.insert(vuln.to_string());
    }

    /// Mark methodology as injected.
    pub fn mark_methodology(&mut self) {
        self.methodology_loaded = true;
    }
}

/// Search result from the wiki.
#[derive(Debug, Clone)]
pub struct KnowledgeResult {
    pub title: String,
    pub layer: String,
    pub content: String,
}

fn layer_tag(layer: KnowledgeLayer) -> &'static str {
    match layer {
        KnowledgeLayer::Technology => "technology",
        KnowledgeLayer::Vulnerability => "vulnerability",
        KnowledgeLayer::Methodology => "methodology",
        KnowledgeLayer::ToolUsage => "tool_usage",
        KnowledgeLayer::ExploitCase => "exploit_case",
        KnowledgeLayer::Remediation => "remediation",
    }
}

/// Search the wiki for a specific topic across specified layers.
/// Returns formatted knowledge fragments ready for system prompt injection.
pub async fn search_wiki(
    pool: &PgPool,
    query: &str,
    layers: &[KnowledgeLayer],
    limit: usize,
) -> Vec<KnowledgeResult> {
    if layers.is_empty() || query.trim().is_empty() {
        return vec![];
    }

    let layer_filters: Vec<String> = layers
        .iter()
        .map(|layer| format!("tags ? '{}'", layer_tag(*layer)))
        .collect();
    let filter_clause = layer_filters.join(" OR ");

    // Build LIKE patterns from query tokens
    let patterns: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == ',' || c == '，')
        .map(str::trim)
        .filter(|t| t.len() >= 2)
        .map(|t| format!("%{t}%"))
        .collect();

    if patterns.is_empty() {
        return vec![];
    }

    let query = format!(
        "SELECT title, tags, content
         FROM pent_knowledge_documents
         WHERE doc_type = 'guide'
           AND ({filter_clause})
           AND (title ILIKE ANY($1) OR content ILIKE ANY($1))
         ORDER BY
           (SELECT COUNT(*) FROM unnest($1::text[]) p WHERE title ILIKE p) DESC,
           updated_at DESC
         LIMIT $2"
    );

    let rows: Vec<(String, Value, String)> = match sqlx::query_as(&query)
        .bind(&patterns)
        .bind(limit as i64)
        .fetch_all(pool)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "wiki search failed");
            return vec![];
        }
    };

    rows.into_iter()
        .map(|(title, tags, content)| {
            let layer = tags
                .as_array()
                .and_then(|arr| {
                    arr.iter().find_map(|t| {
                        let s = t.as_str()?;
                        layers
                            .iter()
                            .find(|layer| layer_tag(**layer) == s)
                            .map(|_| s)
                    })
                })
                .unwrap_or("general");
            KnowledgeResult {
                title,
                layer: layer.to_string(),
                content: if content.chars().count() > 3_000 {
                    format!(
                        "{}...(已截断)",
                        content.chars().take(3_000).collect::<String>()
                    )
                } else {
                    content
                },
            }
        })
        .collect()
}

/// Format knowledge results for system prompt injection.
pub fn format_knowledge(results: &[KnowledgeResult], layer_name: &str) -> String {
    if results.is_empty() {
        return String::new();
    }
    let mut parts = vec![format!("### {} 相关知识", layer_name)];
    for r in results.iter().take(3) {
        parts.push(format!("**{}** ({}):\n{}", r.title, r.layer, r.content));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_deduplicates_technologies() {
        let mut cache = KnowledgeCache::new();
        assert!(cache.needs_technology_search("Apache"));
        cache.mark_technology("Apache");
        assert!(!cache.needs_technology_search("Apache"));
        assert!(cache.needs_technology_search("Nginx"));
    }

    #[test]
    fn methodology_injected_once() {
        let mut cache = KnowledgeCache::new();
        assert!(cache.needs_methodology());
        cache.mark_methodology();
        assert!(!cache.needs_methodology());
    }
}
