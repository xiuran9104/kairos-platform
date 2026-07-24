//! Persistent key-value memory store. Ported from PentAGI, PostgreSQL-backed.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentMemory {
    pub key: String,
    pub value: serde_json::Value,
    pub task_id: Option<String>,
    pub target: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl PersistentMemory {
    pub fn new(
        key: &str,
        value: serde_json::Value,
        task_id: Option<&str>,
        target: Option<&str>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            key: key.to_string(),
            value,
            task_id: task_id.map(String::from),
            target: target.map(String::from),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    cache: HashMap<String, PersistentMemory>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    pub fn get(&self, key: &str) -> Option<&PersistentMemory> {
        self.cache.get(key)
    }
    pub fn set(
        &mut self,
        key: &str,
        value: serde_json::Value,
        task_id: Option<&str>,
        target: Option<&str>,
    ) {
        self.cache.insert(
            key.to_string(),
            PersistentMemory::new(key, value, task_id, target),
        );
    }
    pub fn search_by_task(&self, task_id: &str) -> Vec<&PersistentMemory> {
        self.cache
            .values()
            .filter(|m| m.task_id.as_deref() == Some(task_id))
            .collect()
    }
    pub fn search_by_target(&self, target: &str) -> Vec<&PersistentMemory> {
        self.cache
            .values()
            .filter(|m| m.target.as_deref() == Some(target))
            .collect()
    }
}
