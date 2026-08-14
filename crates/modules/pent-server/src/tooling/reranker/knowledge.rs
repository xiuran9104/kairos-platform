//! Knowledge retrieval reranking — deterministic multi-query expansion and
//! term-overlap scoring.
//!
//! The small-model reranker is a future extension; today the pipeline is
//! deterministic and testable: `expand_queries` broadens recall before search,
//! and `score_guide` ranks candidates by term overlap so the agent sees the
//! most relevant guides first. Callers log the full pre-rerank recall set and
//! return the reranked top-k.

use std::collections::HashSet;

/// Security-oriented synonym groups used to broaden recall. When the query
/// mentions one member of a group, alternate members are emitted as extra
/// queries.
const SYNONYM_GROUPS: &[&[&str]] = &[
    &["sqli", "sql injection", "sql注入", "注入"],
    &["xss", "cross site scripting", "跨站脚本"],
    &["ssrf", "服务端请求伪造"],
    &["idor", "越权", "broken access control"],
    &["csrf", "跨站请求伪造"],
    &["rce", "remote code execution", "命令执行", "代码执行"],
    &["lfi", "local file inclusion", "文件包含"],
    &["xxe", "xml external entity"],
    &["upload", "文件上传", "file upload"],
    &["auth", "认证", "authentication", "鉴权"],
    &["jwt", "json web token"],
];

fn push_unique(out: &mut Vec<String>, candidate: &str) {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return;
    }
    if out.iter().any(|existing| existing.eq_ignore_ascii_case(candidate)) {
        return;
    }
    out.push(candidate.to_string());
}

/// Expand a query into up to `max_queries` deterministic variants: the original
/// query, synonym-group alternates, then significant single tokens.
pub fn expand_queries(query: &str, max_queries: usize) -> Vec<String> {
    let mut out = Vec::new();
    let original = query.trim();
    if original.is_empty() || max_queries == 0 {
        return out;
    }
    push_unique(&mut out, original);
    if out.len() >= max_queries {
        return out;
    }
    let lower = original.to_lowercase();
    for group in SYNONYM_GROUPS {
        let matched: Vec<&str> = group.iter().copied().filter(|term| lower.contains(term)).collect();
        if matched.is_empty() {
            continue;
        }
        for alt in group.iter().copied() {
            if matched.contains(&alt) {
                continue;
            }
            push_unique(&mut out, &lower.replacen(matched[0], alt, 1));
            if out.len() >= max_queries {
                return out;
            }
        }
    }
    for token in original.split(|c: char| !c.is_alphanumeric()) {
        let token = token.trim();
        if token.chars().count() >= 3 {
            push_unique(&mut out, token);
            if out.len() >= max_queries {
                return out;
            }
        }
    }
    out
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 2)
        .collect()
}

fn term_overlap(terms: &HashSet<String>, text: &str) -> f64 {
    let lower = text.to_lowercase();
    terms
        .iter()
        .filter(|term| lower.contains(term.as_str()))
        .count() as f64
}

/// Score a candidate (title + content) against the original query. Title
/// matches are weighted 3x over body matches, matching how the model reads a
/// result card.
pub fn score_guide(query: &str, title: &str, content: &str) -> f64 {
    let terms = tokenize(query);
    if terms.is_empty() {
        return 0.0;
    }
    let title_hits = term_overlap(&terms, title) * 3.0;
    let content_hits = term_overlap(
        &terms,
        &content.chars().take(2_000).collect::<String>(),
    );
    title_hits + content_hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_synonyms_for_sql_injection() {
        let queries = expand_queries("sql injection", 4);
        assert!(queries.iter().any(|q| q.eq_ignore_ascii_case("sql injection")));
        assert!(queries.len() >= 2 && queries.len() <= 4);
    }

    #[test]
    fn deduplicates_variants() {
        let queries = expand_queries("xss", 10);
        let unique: HashSet<&str> = queries.iter().map(String::as_str).collect();
        assert_eq!(queries.len(), unique.len());
    }

    #[test]
    fn empty_query_expands_to_nothing() {
        assert!(expand_queries("  ", 4).is_empty());
        assert!(expand_queries("xss", 0).is_empty());
    }

    #[test]
    fn title_match_scores_higher_than_body_match() {
        let title_score = score_guide("sql injection", "SQL Injection", "generic text");
        let body_score = score_guide("sql injection", "Overview", "a sql injection guide");
        assert!(title_score > body_score);
    }
}
