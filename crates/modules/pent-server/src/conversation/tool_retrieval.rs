//! Candidate retrieval for dynamic tool disclosure.
//! Lexical scoring is always available; FTS and vectors are optional signals.

use std::collections::HashMap;

use kairos_platform_framework_database::PgPool;
use serde_json::Value;

use crate::search_rank::SearchQuery;

#[derive(sqlx::FromRow)]
struct ToolSearchRow {
    tool_id: String,
    namespace: String,
    name: String,
    description: String,
    embedding_document: String,
    tags: Value,
    capabilities: Value,
}

async fn lexical_search(pool: &PgPool, query: &str, top_k: usize) -> Vec<(String, f32)> {
    let search = SearchQuery::new(query);
    if search.is_empty() {
        return Vec::new();
    }
    let rows = sqlx::query_as::<_, ToolSearchRow>(
        "SELECT tool_id, namespace, name, description, embedding_document, tags, capabilities
         FROM pent_tool_definitions
         WHERE lifecycle_status='active'",
    )
    .fetch_all(pool)
    .await;
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "lexical tool search failed");
            return Vec::new();
        }
    };
    let mut ranked = rows
        .into_iter()
        .filter_map(|row| {
            let tags = row.tags.to_string();
            let capabilities = row.capabilities.to_string();
            let score = search.score(&[
                (&row.tool_id, 10.0),
                (&row.name, 8.0),
                (&row.embedding_document, 6.0),
                (&row.description, 5.0),
                (&tags, 4.0),
                (&capabilities, 3.0),
                (&row.namespace, 2.0),
            ]);
            (score > 0.0).then_some((row.tool_id, score))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked.truncate(top_k);
    ranked
}

async fn fts_search(pool: &PgPool, query: &str, top_k: usize) -> Vec<(String, f32)> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let rows = sqlx::query_as::<_, (String, f32)>(
        "SELECT tool_id, ts_rank(
             to_tsvector('simple', coalesce(embedding_document, description)),
             plainto_tsquery('simple', $1)
         ) AS rank
         FROM pent_tool_definitions
         WHERE lifecycle_status='active'
           AND embedding_document IS NOT NULL
           AND embedding_document != ''
         ORDER BY rank DESC
         LIMIT $2",
    )
    .bind(query.chars().take(1000).collect::<String>())
    .bind(top_k as i64)
    .fetch_all(pool)
    .await;
    match rows {
        // `ts_rank` may emit tiny non-zero values (around 1e-20) when a
        // multi-term query has no meaningful match. Treat those as misses so
        // they cannot outrank the CJK-aware lexical signal during RRF merge.
        Ok(rows) => rows
            .into_iter()
            .filter(|(_, rank)| *rank >= 0.001)
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "FTS tool search failed; using lexical retrieval");
            Vec::new()
        }
    }
}

async fn vector_search(pool: &PgPool, query: &str, top_k: usize) -> Vec<(String, f32)> {
    let index = crate::infrastructure::vector::PgVectorIndex::new(pool.clone());
    if !index.has_records().await.unwrap_or(false) {
        return Vec::new();
    }
    let mut embedder = crate::infrastructure::embedding::EmbeddingProvider::new(pool.clone());
    if !embedder.try_configure().await.unwrap_or(false) {
        return Vec::new();
    }
    let Some(embedding) = embedder.embed(query).await else {
        return Vec::new();
    };
    match index.search(&embedding, None, top_k).await {
        Ok(results) => results
            .into_iter()
            .map(|result| (result.tool_id, result.score))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "vector tool search failed; using lexical retrieval");
            Vec::new()
        }
    }
}

fn merge_rankings(rankings: &[Vec<(String, f32)>], top_k: usize) -> Vec<(String, f32)> {
    let mut scores = HashMap::<String, f32>::new();
    for ranking in rankings {
        for (index, (tool_id, _)) in ranking.iter().enumerate() {
            *scores.entry(tool_id.clone()).or_default() += 1.0 / (60.0 + index as f32);
        }
    }
    let mut merged = scores.into_iter().collect::<Vec<_>>();
    merged.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    merged.truncate(top_k);
    merged
}

pub(super) async fn retrieve_tool_ids(
    pool: &PgPool,
    intent: &str,
    top_k: usize,
) -> Vec<(String, f32)> {
    let lexical = lexical_search(pool, intent, top_k * 3).await;
    let fts = fts_search(pool, intent, top_k * 3).await;
    let vectors = vector_search(pool, intent, top_k * 2).await;
    let lexical_count = lexical.len();
    let fts_count = fts.len();
    let vector_count = vectors.len();
    let merged = merge_rankings(&[lexical, fts, vectors], top_k);
    tracing::debug!(
        lexical = lexical_count,
        fts = fts_count,
        vectors = vector_count,
        count = merged.len(),
        intent = %intent.chars().take(50).collect::<String>(),
        "hybrid tool retrieval complete"
    );
    merged
}

#[cfg(test)]
mod tests {
    use super::{merge_rankings, retrieve_tool_ids};

    #[test]
    fn reciprocal_rank_merge_deduplicates_candidates() {
        let merged = merge_rankings(
            &[
                vec![("nuclei".into(), 10.0), ("nmap".into(), 5.0)],
                vec![("nuclei".into(), 0.8), ("sqlmap".into(), 0.7)],
            ],
            3,
        );
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].0, "nuclei");
    }

    #[tokio::test]
    async fn chinese_vulnerability_intent_returns_a_bounded_relevant_set() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };
        let config = kairos_platform_framework_database::DatabaseConfig::new(
            url,
            1,
            2,
            std::time::Duration::from_secs(10),
        )
        .expect("valid test database config");
        let pool = kairos_platform_framework_database::connect(&config)
            .await
            .expect("connect test database");
        let results = retrieve_tool_ids(&pool, "扫描靶场 Web 漏洞", 12).await;
        let names = results
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        assert!(!results.is_empty());
        assert!(results.len() <= 12);
        assert!(
            names.contains(&"nuclei") || names.contains(&"web_vuln_scan"),
            "应召回漏洞扫描工具，实际：{names:?}"
        );
    }
}
