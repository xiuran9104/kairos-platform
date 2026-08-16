//! Hybrid retrieval (FTS + Vector) for tool search.

/// Hybrid retriever combining FTS and vector search results.
#[derive(Default)]
pub struct HybridRetriever;

impl HybridRetriever {
    pub fn new() -> Self {
        Self
    }

    /// Merge FTS and vector search results, de-duplicate by tool_id+version.
    pub fn merge(
        &self,
        fts_results: Vec<RankedTool>,
        vector_results: Vec<RankedTool>,
        top_k: usize,
    ) -> Vec<RankedTool> {
        let mut seen = std::collections::HashSet::new();
        let mut merged = Vec::new();
        for r in fts_results.into_iter().chain(vector_results) {
            let key = format!("{}@{}", r.tool_id, r.version);
            if seen.insert(key) {
                merged.push(r);
            }
        }
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(top_k);
        merged
    }
}

#[derive(Debug, Clone)]
pub struct RankedTool {
    pub tool_id: String,
    pub version: String,
    pub score: f32,
    pub source: String, // "fts" or "vector"
}
