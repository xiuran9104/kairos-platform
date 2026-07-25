//! Auto-memory: records tool results and searches knowledge during Worker execution.
//!
//! Three knowledge types map to PentAGI's model:
//!   - memory: auto-recorded tool execution facts, per-flow
//!   - guide: stored methodology guides (attack techniques, Q&A)
//!   - code: stored code snippets and exploits

use super::embedding::EmbeddingProvider;
use super::knowledge::{InsertKnowledgeParams, KnowledgeDocType, KnowledgeFilter, KnowledgeStore};
use serde_json::{Value, json};
use uuid::Uuid;

pub struct AutoMemory {
    store: KnowledgeStore,
    embedder: Option<EmbeddingProvider>,
    flow_id: Option<i32>,
}

impl AutoMemory {
    pub fn new(store: KnowledgeStore, embedder: Option<EmbeddingProvider>) -> Self {
        Self {
            store,
            embedder,
            flow_id: None,
        }
    }

    pub fn with_flow(mut self, flow_id: i32) -> Self {
        self.flow_id = Some(flow_id);
        self
    }

    /// Record a tool execution result as a memory fact.
    pub async fn record_tool_run(
        &self,
        tool_name: &str,
        target: &str,
        summary: &str,
        exit_code: i32,
        duration_ms: u64,
        task_id: Option<i32>,
        subtask_id: Option<i32>,
    ) -> Result<Uuid, String> {
        let doc_id = self
            .store
            .insert(InsertKnowledgeParams {
                doc_type: KnowledgeDocType::Answer, // runtime facts stored as "answer" type
                title: format!("{} on {}", tool_name, target),
                content: format!(
                    "Tool: {}\nTarget: {}\nExit: {}\nDuration: {}ms\n\n{}",
                    tool_name, target, exit_code, duration_ms, summary
                ),
                source: Some("auto-memory".to_string()),
                tags: json!(["memory", "tool_run", tool_name]),
                flow_id: self.flow_id,
                task_id,
                subtask_id,
                question: Some(format!(
                    "what happened when {} ran on {}?",
                    tool_name, target
                )),
                description: Some(format!(
                    "Execution result of {} against {}",
                    tool_name, target
                )),
                guide_type: None,
                answer_type: Some("tool_result".into()),
                code_lang: None,
                manual: false,
            })
            .await?;

        // Try to embed if provider available
        if let Some(ref embedder) = self.embedder {
            if embedder.is_available() {
                if let Some(embedding) = embedder.embed(&summary).await {
                    let _ = self
                        .store
                        .upsert_embedding(
                            doc_id,
                            &embedding,
                            &json!({
                                "tool_name": tool_name,
                                "target": target,
                                "flow_id": self.flow_id,
                                "task_id": task_id,
                                "subtask_id": subtask_id,
                            }),
                        )
                        .await;
                }
            }
        }

        tracing::debug!(tool=%tool_name, target=%target, doc_id=%doc_id, "memory recorded");
        Ok(doc_id)
    }

    /// Search memory for relevant facts about a target/tool.
    pub async fn recall(&self, query: &str, task_id: Option<i32>, limit: usize) -> Vec<String> {
        let filter = KnowledgeFilter {
            doc_type: Some("answer".into()),
            flow_id: self.flow_id,
            task_id,
            ..Default::default()
        };

        // Try vector search first
        if let Some(ref embedder) = self.embedder {
            if embedder.is_available() {
                if let Some(embedding) = embedder.embed(query).await {
                    if let Ok(results) = self.store.search(&embedding, &filter, limit).await {
                        return results
                            .into_iter()
                            .map(|r| format!("[score:{:.2}] {}", r.score, r.document.content))
                            .collect();
                    }
                }
            }
        }

        // Fallback to text search
        self.store
            .search_text(query, &filter, limit)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|d| d.content)
            .collect()
    }

    /// Search guides for methodology/attack patterns.
    pub async fn search_guides(
        &self,
        query: &str,
        guide_type: Option<&str>,
        limit: usize,
    ) -> Vec<String> {
        let filter = KnowledgeFilter {
            doc_type: Some("guide".into()),
            guide_type: guide_type.map(String::from),
            ..Default::default()
        };

        self.store
            .search_text(query, &filter, limit)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|d| d.content)
            .collect()
    }

    /// Store a code snippet found during testing.
    pub async fn store_code(
        &self,
        title: &str,
        code: &str,
        lang: &str,
        task_id: Option<i32>,
    ) -> Result<Uuid, String> {
        self.store
            .insert(InsertKnowledgeParams {
                doc_type: KnowledgeDocType::Code,
                title: title.into(),
                content: code.into(),
                source: Some("auto-memory".to_string()),
                tags: json!(["code", lang]),
                flow_id: self.flow_id,
                task_id,
                subtask_id: None,
                question: None,
                description: Some(format!("Code snippet: {}", title)),
                guide_type: None,
                answer_type: None,
                code_lang: Some(lang.into()),
                manual: false,
            })
            .await
    }
}
