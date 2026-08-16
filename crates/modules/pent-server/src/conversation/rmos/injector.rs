//! RMOS Injector — auto-injects Memory + Knowledge + Attack Graph
//! into the conversation system prompt before each model turn.
//!
//! Called at the start of every tool-loop round. Only injects new
//! knowledge (cache-miss), always injects current memory state.

use super::attack_graph::AttackGraph;
use super::knowledge_cache::{self, KnowledgeCache};
use super::memory::PentestMemory;
use kairos_platform_pent_runtime::ports::{
    RuntimeEnrichment, RuntimeKnowledgeLayer as KnowledgeLayer,
};

/// What gets injected into the system prompt for the next turn.
pub struct Injection {
    /// Structured pentest memory summary.
    pub memory_summary: String,
    /// New knowledge to inject (only on cache miss).
    pub knowledge_blocks: Vec<String>,
    /// Attack chain visualization.
    pub attack_chain: String,
    /// Whether anything meaningful was injected.
    pub has_content: bool,
}

impl Injection {
    pub fn empty() -> Self {
        Self {
            memory_summary: String::new(),
            knowledge_blocks: Vec::new(),
            attack_chain: String::new(),
            has_content: false,
        }
    }
}

/// Compute the injection for the next turn.
pub async fn prepare_injection(
    enrichment: &dyn RuntimeEnrichment,
    memory: &PentestMemory,
    cache: &mut KnowledgeCache,
    graph: &AttackGraph,
) -> Injection {
    let mut injection = Injection::empty();

    // ── Memory: always inject current state ──
    let memory_summary = memory.to_context_summary();
    if !memory_summary.is_empty() {
        injection.memory_summary = memory_summary;
        injection.has_content = true;
    }

    // ── Knowledge: only inject new discoveries ──
    // New technologies → search technology + exploit_case layers
    for tech in &memory.technologies {
        if cache.needs_technology_search(tech) {
            let results = search_knowledge(
                enrichment,
                tech,
                &KnowledgeLayer::layers_for_technology(),
                3,
            )
            .await;
            if !results.is_empty() {
                injection
                    .knowledge_blocks
                    .push(knowledge_cache::format_knowledge(
                        &results,
                        &format!("技术栈: {tech}"),
                    ));
                injection.has_content = true;
            }
            cache.mark_technology(tech);
        }
    }

    // New vulnerability types → search vulnerability + exploit_case + remediation
    let vuln_types: std::collections::HashSet<String> = memory
        .findings
        .iter()
        .map(|f| f.vuln_type.clone())
        .collect();

    for vuln in &vuln_types {
        if cache.needs_vulnerability_search(vuln) {
            let results = search_knowledge(
                enrichment,
                vuln,
                &KnowledgeLayer::layers_for_vulnerability(),
                3,
            )
            .await;
            if !results.is_empty() {
                injection
                    .knowledge_blocks
                    .push(knowledge_cache::format_knowledge(
                        &results,
                        &format!("漏洞: {vuln}"),
                    ));
                injection.has_content = true;
            }
            cache.mark_vulnerability(vuln);
        }
    }

    // Methodology: inject once per session, including full vulnerability checklist
    if cache.needs_methodology() {
        // First inject general methodology
        let results = search_knowledge(
            enrichment,
            "penetration testing methodology web application",
            &KnowledgeLayer::layers_for_methodology(),
            3,
        )
        .await;
        if !results.is_empty() {
            injection
                .knowledge_blocks
                .push(knowledge_cache::format_knowledge(&results, "渗透方法论"));
            injection.has_content = true;
        }

        // Pre-fetch methodology for ALL common vulnerability types
        // so the LLM knows what to test BEFORE finding anything.
        let preload_types = [
            "idor",
            "jwt",
            "ssrf",
            "xss",
            "sql injection",
            "access control",
            "file upload",
            "path traversal",
            "csrf",
            "sensitive data exposure",
            "security misconfiguration",
        ];
        let mut preload_results: Vec<String> = Vec::new();
        for vuln_type in &preload_types {
            let results = search_knowledge(
                enrichment,
                vuln_type,
                &[KnowledgeLayer::Vulnerability, KnowledgeLayer::ExploitCase],
                2,
            )
            .await;
            for r in results {
                let summary = format!(
                    "**{}** ({})\n{}",
                    r.title,
                    r.layer,
                    r.content.chars().take(500).collect::<String>()
                );
                if !preload_results.contains(&summary) {
                    preload_results.push(summary);
                }
            }
            // Mark as loaded so we don't re-search later
            cache.mark_vulnerability(vuln_type);
        }
        if !preload_results.is_empty() {
            let combined = preload_results.join("\n\n");
            injection.knowledge_blocks.push(format!(
                "### 漏洞检测方法速查\n\n以下方法应在渗透测试中逐一覆盖：\n\n{combined}"
            ));
            injection.has_content = true;
        }
        cache.mark_methodology();
    }

    // ── Attack Graph: inject if there are nodes ──
    let attack_chain = graph.to_chain_summary();
    if !attack_chain.is_empty() {
        injection.attack_chain = attack_chain;
        injection.has_content = true;
    }

    injection
}

async fn search_knowledge(
    enrichment: &dyn RuntimeEnrichment,
    query: &str,
    layers: &[KnowledgeLayer],
    limit: usize,
) -> Vec<knowledge_cache::KnowledgeResult> {
    enrichment
        .search_context_knowledge(query, layers, limit)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|result| knowledge_cache::KnowledgeResult {
            title: result.title,
            layer: result.layer,
            content: result.content,
        })
        .collect()
}

/// Build the full injected content to append to the system prompt.
pub fn build_injected_context(injection: &Injection) -> String {
    if !injection.has_content {
        return String::new();
    }

    let mut parts: Vec<String> = Vec::new();

    if !injection.memory_summary.is_empty() {
        parts.push(injection.memory_summary.clone());
    }

    if !injection.knowledge_blocks.is_empty() {
        parts.push("\n## 相关知识库".to_string());
        for block in &injection.knowledge_blocks {
            parts.push(block.clone());
        }
    }

    if !injection.attack_chain.is_empty() {
        parts.push(injection.attack_chain.clone());
    }

    parts.join("\n")
}
