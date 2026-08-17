//! Local builtin tool implementations — read, write, glob, grep, web_search.

use anyhow::Context;
use regex::Regex;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

/// Read a file from the local filesystem. Supports offset and limit for pagination.
pub async fn execute_read(args: Value) -> anyhow::Result<Value> {
    let file_path = required_string(&args, "file_path")?;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

    let path = Path::new(&file_path);
    anyhow::ensure!(path.exists(), "文件不存在: {file_path}");
    anyhow::ensure!(path.is_file(), "路径不是文件: {file_path}");

    // Security: only allow absolute paths within allowed roots
    let canonical = path.canonicalize()?;
    let allowed_roots = allowed_filesystem_roots();
    anyhow::ensure!(
        allowed_roots.iter().any(|root| canonical.starts_with(root)),
        "文件路径不在允许的根目录中: {file_path}"
    );

    let content = tokio::fs::read_to_string(&canonical).await?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let selected: Vec<&str> = lines.iter().skip(offset).take(limit).copied().collect();

    Ok(json!({
        "content": selected.join("\n"),
        "total_lines": total_lines,
        "offset": offset,
        "lines_read": selected.len(),
        "file_path": canonical.to_string_lossy(),
    }))
}

/// Search text files for a regex pattern without spawning a host process.
pub async fn execute_grep(args: Value) -> anyhow::Result<Value> {
    let pattern = required_string(&args, "pattern")?;
    let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let context_lines = args
        .get("context_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let canonical = canonical_allowed_path(search_path)?;
    let regex = Regex::new(&pattern).context("无效正则表达式")?;
    let lines = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || search_text_files(&canonical, &regex, context_lines)),
    )
    .await
    .context("grep 超时")??;

    Ok(json!({
        "matches": lines,
        "match_count": lines.len(),
        "exit_code": if lines.is_empty() { 1 } else { 0 },
        "stderr": "",
    }))
}

/// Search the web using DuckDuckGo Instant Answer API (no HTML parsing needed).
pub async fn execute_web_search(args: Value) -> anyhow::Result<Value> {
    let query = required_string(&args, "query")?;
    let search_url = std::env::var("PENTA_SEARCH_ENDPOINT")
        .unwrap_or_else(|_| "https://api.duckduckgo.com/".into());
    let client = http_client()?;

    let resp = client
        .get(&search_url)
        .query(&[
            ("q", query.as_str()),
            ("format", "json"),
            ("no_html", "1"),
            ("skip_disambig", "1"),
        ])
        .send()
        .await?;
    let body: Value = resp.json().await?;

    // DuckDuckGo JSON API returns results in several fields
    let mut results = Vec::new();

    // Abstract / main answer
    if let Some(text) = body.get("AbstractText").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            let url = body
                .get("AbstractURL")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            results.push(json!({"title": text.chars().take(300).collect::<String>(), "url": url, "source": "abstract"}));
        }
    }

    // Related topics
    if let Some(items) = body.pointer("/RelatedTopics").and_then(|v| v.as_array()) {
        for item in items.iter().take(10) {
            let title = item
                .get("Text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();
            let url = item.get("FirstURL").and_then(|v| v.as_str()).unwrap_or("");
            if !title.is_empty() {
                results.push(json!({"title": title, "url": url, "source": "related"}));
            }
        }
    }

    // Results (web links)
    if let Some(items) = body.get("Results").and_then(|v| v.as_array()) {
        for item in items.iter().take(10) {
            let title = item
                .get("Text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();
            let url = item.get("FirstURL").and_then(|v| v.as_str()).unwrap_or("");
            if !title.is_empty() {
                results.push(json!({"title": title, "url": url, "source": "web"}));
            }
        }
    }

    Ok(json!({
        "query": query,
        "results": results,
        "total_results": results.len(),
        "heading": body.get("Heading").and_then(|v| v.as_str()).unwrap_or(""),
    }))
}

/// Write content to a file. Creates parent directories if needed.
/// Overwrites existing files.
pub async fn execute_write(args: Value) -> anyhow::Result<Value> {
    let file_path = required_string(&args, "file_path")?;
    let content = required_string(&args, "content")?;

    let path = Path::new(&file_path);
    anyhow::ensure!(!path.is_dir(), "路径是目录而非文件: {file_path}");

    // Security: only allow absolute paths within allowed roots
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("创建父目录失败")?;
    }

    // If file exists, canonicalize to verify it's within allowed roots
    if path.exists() {
        let canonical = path.canonicalize()?;
        let allowed_roots = allowed_filesystem_roots();
        anyhow::ensure!(
            allowed_roots.iter().any(|root| canonical.starts_with(root)),
            "文件路径不在允许的根目录中: {file_path}"
        );
        tokio::fs::write(&canonical, &content).await?;
        Ok(
            json!({"written": true, "file_path": canonical.to_string_lossy(), "bytes": content.len()}),
        )
    } else {
        // New file: normalize the parent directory so `..` cannot escape the
        // allowed roots, then verify the canonical parent is still within one.
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let file_name = absolute
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("无效文件路径: {file_path}"))?;
        let parent = absolute.parent().unwrap_or_else(|| Path::new("/"));
        tokio::fs::create_dir_all(parent)
            .await
            .context("创建父目录失败")?;
        let canonical_parent = parent.canonicalize().context("解析父目录失败")?;
        let allowed_roots = allowed_filesystem_roots();
        anyhow::ensure!(
            allowed_roots
                .iter()
                .any(|root| canonical_parent.starts_with(root)),
            "文件路径不在允许的根目录中: {file_path}"
        );
        let target = canonical_parent.join(file_name);
        tokio::fs::write(&target, &content).await?;
        Ok(json!({"written": true, "file_path": target.to_string_lossy(), "bytes": content.len()}))
    }
}

/// Find files matching a glob pattern without spawning a host process.
/// Simple patterns like `*.rs` match filenames; patterns with `/` match full paths.
pub async fn execute_glob(args: Value) -> anyhow::Result<Value> {
    let pattern = required_string(&args, "pattern")?;
    let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    let canonical = canonical_allowed_path(search_path)?;
    let match_paths = pattern.contains('/');
    let pattern_for_search = pattern.clone();
    let matches = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            find_matching_files(&canonical, &pattern_for_search, match_paths)
        }),
    )
    .await
    .context("文件搜索超时")??;

    Ok(json!({
        "pattern": pattern,
        "path": search_path,
        "matches": matches,
        "match_count": matches.len(),
        "stderr": "",
    }))
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn required_string(args: &Value, field: &str) -> anyhow::Result<String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("缺少必需参数: {field}"))
}

fn allowed_filesystem_roots() -> Vec<std::path::PathBuf> {
    let mut roots = vec![std::env::current_dir().unwrap_or_else(|_| Path::new("/").to_path_buf())];
    if let Ok(extra) = std::env::var("PENTA_LOCAL_TOOLS_ALLOWED_ROOTS") {
        for root in extra.split(',') {
            roots.push(Path::new(root.trim()).to_path_buf());
        }
    }
    roots
}

fn canonical_allowed_path(path: &str) -> anyhow::Result<std::path::PathBuf> {
    let canonical = Path::new(path).canonicalize()?;
    let allowed = allowed_filesystem_roots()
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| canonical.starts_with(root));
    anyhow::ensure!(allowed, "搜索路径不在允许的根目录中: {path}");
    Ok(canonical)
}

fn visit_files(root: &Path, max_depth: usize, visitor: &mut impl FnMut(&Path)) {
    fn walk(path: &Path, depth: usize, max_depth: usize, visitor: &mut impl FnMut(&Path)) {
        if depth > max_depth {
            return;
        }
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return;
        };
        if metadata.file_type().is_symlink() {
            return;
        }
        if metadata.is_file() {
            visitor(path);
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            walk(&entry.path(), depth + 1, max_depth, visitor);
        }
    }
    walk(root, 0, max_depth, visitor);
}

fn search_text_files(root: &Path, regex: &Regex, context: usize) -> Vec<String> {
    const TEXT_EXTENSIONS: &[&str] = &[
        "txt", "md", "json", "html", "js", "ts", "rs", "py", "sh", "yml", "yaml", "toml", "cfg",
        "ini", "conf",
    ];
    let mut matches = Vec::new();
    visit_files(root, 40, &mut |path| {
        if matches.len() >= 200
            || !path
                .extension()
                .and_then(|v| v.to_str())
                .is_some_and(|ext| TEXT_EXTENSIONS.contains(&ext))
        {
            return;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        let lines: Vec<_> = content.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                let start = index.saturating_sub(context);
                let end = (index + context + 1).min(lines.len());
                for (offset, value) in lines[start..end].iter().enumerate() {
                    matches.push(format!(
                        "{}:{}:{}",
                        path.display(),
                        start + offset + 1,
                        value
                    ));
                    if matches.len() >= 200 {
                        return;
                    }
                }
            }
        }
    });
    matches
}

fn find_matching_files(root: &Path, pattern: &str, match_paths: bool) -> Vec<String> {
    let regex = Regex::new(&format!(
        "^{}$",
        regex::escape(pattern)
            .replace(r"\*", ".*")
            .replace(r"\?", ".")
    ))
    .expect("escaped glob is valid regex");
    let mut matches = Vec::new();
    visit_files(root, 40, &mut |path| {
        if matches.len() >= 500 {
            return;
        }
        let candidate = if match_paths {
            path.strip_prefix(root).unwrap_or(path).to_string_lossy()
        } else {
            path.file_name().unwrap_or_default().to_string_lossy()
        };
        if regex.is_match(&candidate) {
            matches.push(path.to_string_lossy().into_owned());
        }
    });
    matches
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Kairos-Pent-Agent/1.0")
        .build()
        .context("创建 HTTP 客户端失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grep_and_glob_use_confined_rust_traversal() {
        let cwd = std::env::current_dir().expect("current dir");
        let root = cwd.join(format!("kairos-search-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).expect("create test directory");
        std::fs::write(root.join("sample.rs"), "alpha\nneedle-42\nomega\n").expect("write fixture");

        let grep = execute_grep(json!({
            "pattern": "needle-[0-9]+",
            "path": root.to_string_lossy()
        }))
        .await
        .expect("grep result");
        assert_eq!(grep["match_count"], 1);
        assert!(grep["matches"][0].as_str().unwrap().contains("needle-42"));

        let glob = execute_glob(json!({
            "pattern": "*.rs",
            "path": root.to_string_lossy()
        }))
        .await
        .expect("glob result");
        assert_eq!(glob["match_count"], 1);
        assert!(glob["matches"][0].as_str().unwrap().ends_with("sample.rs"));

        std::fs::remove_dir_all(root).expect("remove test directory");
    }

    #[tokio::test]
    async fn write_rejects_parent_directory_traversal() {
        let cwd = std::env::current_dir().expect("current dir");

        // Attempting to escape the allowed root via `..` must fail.
        let escape = cwd.join("..").join("kairos-escape-test.txt");
        let result = execute_write(json!({
            "file_path": escape.to_string_lossy(),
            "content": "evil"
        }))
        .await;
        assert!(result.is_err(), "父目录穿越应该被拒绝");

        // A normal write inside the allowed root must succeed.
        let inside = cwd.join("kairos-inside-test.txt");
        let result = execute_write(json!({
            "file_path": inside.to_string_lossy(),
            "content": "ok"
        }))
        .await;
        assert!(result.is_ok());
        let _ = std::fs::remove_file(&inside);
    }
}
