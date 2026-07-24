//! Knowledge base — PostgreSQL-native implementation (ported from PentAGI).
//!
//! Provides:
//! - Document CRUD with full-text search (PostgreSQL tsvector)
//! - Vector embeddings (pgvector compatible)
//! - Knowledge graph entities
//! - Conversation memory

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Document management ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    pub content: String,
    pub source: String,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document: KnowledgeDocument,
    pub score: f32,
    pub snippet: String,
}

// ── Knowledge graph entities ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntity {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub properties: serde_json::Value,
    pub relations: Vec<EntityRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelation {
    pub target_id: String,
    pub relation_type: String,
    pub weight: f32,
}

// ── Conversation memory ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationMemory {
    pub entries: Vec<MemoryEntry>,
    pub max_entries: usize,
    pub summary: Option<String>,
}

impl ConversationMemory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
            summary: None,
        }
    }

    pub fn add(&mut self, role: &str, content: &str) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(MemoryEntry {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            timestamp: Utc::now(),
        });
    }

    pub fn add_tool_call(&mut self, role: &str, content: &str, tool_calls: serde_json::Value) {
        self.entries.push(MemoryEntry {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: Some(tool_calls),
            timestamp: Utc::now(),
        });
    }

    /// Build context string for LLM prompt assembly.
    pub fn build_context(&self) -> String {
        let mut ctx = String::new();
        if let Some(ref summary) = self.summary {
            ctx.push_str(&format!("## 历史摘要\n{}\n\n", summary));
        }
        ctx.push_str("## 对话记录\n");
        for entry in &self.entries {
            ctx.push_str(&format!(
                "[{}] {}: {}\n",
                entry.timestamp.format("%H:%M:%S"),
                entry.role,
                entry.content
            ));
        }
        ctx
    }

    /// Summarize old entries to save context window space.
    pub fn summarize_old_entries(&mut self, keep_recent: usize, summary: String) {
        if self.entries.len() > keep_recent {
            let old_count = self.entries.len() - keep_recent;
            self.entries.drain(0..old_count);
            self.summary = Some(summary);
        }
    }
}

// ── Knowledge platform config (from PentAGI) ────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgePlatformConfig {
    pub local: LocalKnowledgeConfig,
    pub graph: KnowledgeGraphConfig,
    pub vector: VectorStoreConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalKnowledgeConfig {
    pub enabled: bool,
    pub wiki_dir: Option<String>,
    pub index_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeGraphConfig {
    pub enabled: bool,
    pub backend: String, // "pgvector" or "graphiti"
    pub connection_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VectorStoreConfig {
    pub enabled: bool,
    pub dimensions: usize,
    pub index_type: String,
}

// ── Wiki index status (from PentAGI) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiIndexStatus {
    pub total_documents: usize,
    pub indexed_documents: usize,
    pub last_indexed_at: Option<DateTime<Utc>>,
    pub index_path: String,
}
