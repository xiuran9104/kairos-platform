//! Per-session file cache with mtime-based invalidation.
//!
//! Prevents redundant reads of large files (scan outputs, HTML responses,
//! evidence artifacts) within a single conversation turn. External
//! modifications are detected via mtime comparison and evict stale entries.
//!
//! Pattern borrowed from PentAGI's `FileCache`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

/// Maximum cached file size in bytes (1 MB).
const MAX_CACHE_ENTRY_BYTES: u64 = 1_024 * 1_024;

#[derive(Debug, Clone)]
struct CachedEntry {
    content: String,
    mtime: SystemTime,
    size: u64,
}

/// Thread-safe per-session file cache.
///
/// Call [`FileCache::read`] before hitting disk for any file that may be
/// read multiple times within a turn. The cache checks mtime on every access;
/// stale entries are evicted transparently.
#[derive(Debug, Default)]
pub struct FileCache {
    entries: RwLock<HashMap<PathBuf, CachedEntry>>,
}

impl FileCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read a file, returning the cached content if the mtime hasn't changed.
    /// Returns `None` if the file doesn't exist or can't be read.
    pub fn read(&self, path: &Path) -> Option<String> {
        // Fast path: cache hit with matching mtime
        {
            let entries = self.entries.read().unwrap();
            if let Some(entry) = entries.get(path) {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime == entry.mtime && meta.len() == entry.size {
                            return Some(entry.content.clone());
                        }
                    }
                }
                // mtime mismatch — entry is stale, fall through to reload
            }
        }

        // Slow path: read from disk
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > MAX_CACHE_ENTRY_BYTES {
            // File too large — read but don't cache
            return std::fs::read_to_string(path).ok();
        }
        let mtime = meta.modified().ok()?;
        let content = std::fs::read_to_string(path).ok()?;
        let size = meta.len();

        let mut entries = self.entries.write().unwrap();
        entries.insert(
            path.to_path_buf(),
            CachedEntry {
                content: content.clone(),
                mtime,
                size,
            },
        );
        Some(content)
    }

    /// Invalidate a specific path from the cache.
    pub fn invalidate(&self, path: &Path) {
        self.entries.write().unwrap().remove(path);
    }

    /// Invalidate all entries whose path contains the given substring.
    /// Called after write/edit operations that may affect multiple files.
    pub fn invalidate_pattern(&self, pattern: &str) {
        let mut entries = self.entries.write().unwrap();
        entries.retain(|path, _| !path.to_string_lossy().contains(pattern));
    }

    /// Remove all cached entries.
    pub fn clear(&self) {
        self.entries.write().unwrap().clear();
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_and_returns_file_content() {
        let cache = FileCache::new();
        let path = std::env::temp_dir().join(format!("fc_test_{}.txt", std::process::id()));
        std::fs::write(&path, "hello").unwrap();

        let first = cache.read(&path).unwrap();
        assert_eq!(first, "hello");
        assert_eq!(cache.len(), 1);

        let second = cache.read(&path).unwrap();
        assert_eq!(second, "hello");
        assert_eq!(cache.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evicts_stale_entry_on_mtime_change() {
        let cache = FileCache::new();
        let path = std::env::temp_dir().join(format!("fc_stale_{}.txt", std::process::id()));
        std::fs::write(&path, "v1").unwrap();

        assert_eq!(cache.read(&path).unwrap(), "v1");
        assert_eq!(cache.len(), 1);

        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&path, "v2").unwrap();

        assert_eq!(cache.read(&path).unwrap(), "v2");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalidate_removes_entry() {
        let cache = FileCache::new();
        let path = std::env::temp_dir().join(format!("fc_inv_{}.txt", std::process::id()));
        std::fs::write(&path, "data").unwrap();

        cache.read(&path);
        assert_eq!(cache.len(), 1);

        cache.invalidate(&path);
        assert_eq!(cache.len(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_removes_all() {
        let cache = FileCache::new();
        let path = std::env::temp_dir().join(format!("fc_clear_{}.txt", std::process::id()));
        std::fs::write(&path, "x").unwrap();

        cache.read(&path);
        cache.clear();
        assert!(cache.is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
