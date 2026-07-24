//! 文件读取缓存模块，提供会话级别的文件内容缓存，避免 LLM 在同一工具循环中重复读取相同文件。
//!
//! 核心特性：
//! - **mtime 校验**：每次访问时检查文件的修改时间，若被外部修改则自动淘汰缓存条目
//! - **读写一致性**：写入/编辑操作会自动刷新缓存，确保后续读取反映最新的工具可见状态
//! - **克隆隔离**：`Clone` 实现产生空缓存，避免快照会话共享缓存

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Per-session cache that prevents the model from re-reading the same file
/// within a single LLM tool loop. Write and edit tools refresh cached entries so
/// reads always reflect the latest tool-visible state after a mutation.
///
/// The cache also checks the file's `mtime` on access — if the file was
/// modified externally (e.g. by a background bash task or the user), the
/// stale entry is discarded and a fresh read hits disk.
#[derive(Debug, Default)]
pub struct FileCache {
    entries: Mutex<HashMap<PathBuf, CachedRead>>,
    read_states: Mutex<HashMap<PathBuf, FileReadState>>,
}

impl Clone for FileCache {
    /// Each clone starts with an empty cache. This is intentional:
    /// a cloned Session represents a snapshot and should not share
    /// cached reads with the original.
    fn clone(&self) -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
struct CachedRead {
    content: Vec<u8>,
    mtime: SystemTime,
}

#[derive(Debug, Clone)]
pub struct FileReadState {
    pub content: String,
    pub mtime: SystemTime,
    pub is_partial_view: bool,
}

impl FileCache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            read_states: Mutex::new(HashMap::new()),
        }
    }

    /// Return cached content if the file on disk has the same mtime as when
    /// it was cached. Returns `None` on cache miss or stale entry.
    pub fn get(&self, path: &Path) -> Option<Vec<u8>> {
        let current_mtime = std::fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())?;
        let mut entries = self.entries.lock().unwrap();
        let cached = entries.get(path)?;
        if current_mtime == cached.mtime {
            Some(cached.content.clone())
        } else {
            entries.remove(path);
            None
        }
    }

    /// Store file content keyed by its canonical path, recording the
    /// current mtime so we can detect external modifications later.
    pub fn set(&self, path: PathBuf, content: impl Into<Vec<u8>>) {
        let mtime = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            path,
            CachedRead {
                content: content.into(),
                mtime,
            },
        );
    }

    pub fn record_read(&self, path: PathBuf, content: String, is_partial_view: bool) {
        let mtime = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut read_states = self.read_states.lock().unwrap();
        read_states.insert(
            path,
            FileReadState {
                content,
                mtime,
                is_partial_view,
            },
        );
    }

    pub fn read_state(&self, path: &Path) -> Option<FileReadState> {
        let current_mtime = std::fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())?;
        let mut read_states = self.read_states.lock().unwrap();
        let state = read_states.get(path)?;
        if current_mtime == state.mtime {
            Some(state.clone())
        } else {
            read_states.remove(path);
            None
        }
    }

    pub fn update_after_write(&self, path: PathBuf, content: String) {
        self.set(path.clone(), content.as_bytes().to_vec());
        self.record_read(path, content, false);
    }

    /// Remove a path from the cache (called after write / edit operations).
    pub fn invalidate(&self, path: &Path) {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(path);
        let mut read_states = self.read_states.lock().unwrap();
        read_states.remove(path);
    }

    /// Remove all cached entries whose path starts with `parent`.
    pub fn invalidate_dir(&self, _parent: &Path) {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|cached_path, _| !cached_path.starts_with(_parent));
        let mut read_states = self.read_states.lock().unwrap();
        read_states.retain(|cached_path, _| !cached_path.starts_with(_parent));
    }

    /// Number of cached entries (useful for testing / debugging).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn cache_hit_and_miss() {
        let tmp = std::env::temp_dir().join(format!("pentagi-fc-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("a.txt");
        std::fs::write(&path, "hello").unwrap();

        let cache = FileCache::new();
        assert!(cache.get(&path).is_none(), "cold cache miss");

        cache.set(path.clone(), b"hello".to_vec());
        assert_eq!(
            cache.get(&path).as_deref(),
            Some(b"hello".as_slice()),
            "warm cache hit"
        );
        assert_eq!(cache.len(), 1);

        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn stale_entry_on_external_modification() {
        let tmp = std::env::temp_dir().join(format!("pentagi-fc-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("b.txt");
        std::fs::write(&path, "v1").unwrap();

        let cache = FileCache::new();
        cache.set(path.clone(), b"v1".to_vec());
        assert_eq!(cache.get(&path).as_deref(), Some(b"v1".as_slice()));

        // Simulate external modification
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&path, "v2").unwrap();

        assert!(cache.get(&path).is_none(), "stale entry evicted");

        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn invalidate_removes_entry() {
        let tmp = std::env::temp_dir().join(format!("pentagi-fc-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("c.txt");
        std::fs::write(&path, "data").unwrap();

        let cache = FileCache::new();
        cache.set(path.clone(), b"data".to_vec());
        assert_eq!(cache.len(), 1);

        cache.invalidate(&path);
        assert_eq!(cache.len(), 0);

        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn invalidate_dir_clears_subtree() {
        let tmp = std::env::temp_dir().join(format!("pentagi-fc-{}", Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        let a = tmp.join("a.txt");
        let b = tmp.join("sub/b.txt");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();

        let cache = FileCache::new();
        cache.set(a.clone(), b"a".to_vec());
        cache.set(b.clone(), b"b".to_vec());
        assert_eq!(cache.len(), 2);

        cache.invalidate_dir(&tmp.join("sub"));
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&a).is_some());
        assert!(cache.get(&b).is_none());

        let _ = std::fs::remove_dir_all(tmp);
    }
}
