//! Small-model reranker (AI-06). Takes ≤8 candidates, returns ≤3.

use kairos_platform_pent_api::ToolRouteCandidate;

pub mod knowledge;

pub struct SmallModelReranker;

impl SmallModelReranker {
    /// Rerank candidates using a small model. Falls back to deterministic scoring.
    pub async fn rerank(&self, candidates: &[ToolRouteCandidate]) -> Vec<ToolRouteCandidate> {
        let mut ranked: Vec<_> = candidates.iter().take(8).cloned().collect();
        // Sort by deterministic score — small model reranking would go here
        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(3);
        ranked
    }

    /// Health check — small model availability.
    pub fn is_available(&self) -> bool {
        // Check if the local Ollama/vLLM is running
        std::env::var("PENTA_RERANKER_URL").is_ok()
    }
}
