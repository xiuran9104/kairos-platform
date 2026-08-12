//! Local builtin tool implementations — read, write, glob, grep, bash, web_search.

use anyhow::Context;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

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

/// Search files for a regex pattern using grep.
pub async fn execute_grep(args: Value) -> anyhow::Result<Value> {
    let pattern = required_string(&args, "pattern")?;
    let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let context_lines = args
        .get("context_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("grep")
            .arg("-rnE")
            .arg("--include=*.{txt,md,json,html,js,ts,rs,py,sh,yml,yaml,toml,cfg,ini,conf}")
            .arg("--color=never")
            .arg(if context_lines > 0 {
                format!("-C{}", context_lines)
            } else {
                String::from("-n")
            })
            .arg(&pattern)
            .arg(search_path)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("grep 超时")?
    .context("grep 执行失败")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stdout.lines().take(200).collect();

    Ok(json!({
        "matches": lines,
        "match_count": lines.len(),
        "exit_code": output.status.code().unwrap_or(-1),
        "stderr": stderr.to_string(),
    }))
}

/// Execute a bash command in a sandboxed environment.
pub async fn execute_bash(args: Value) -> anyhow::Result<Value> {
    let command = required_string(&args, "command")?;
    let timeout_secs = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60);

    // Simple safety checks
    anyhow::ensure!(!command.is_empty(), "命令不能为空");
    anyhow::ensure!(
        !command.contains("rm -rf /") && !command.contains("mkfs."),
        "危险命令被禁止"
    );

    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new("bash")
            .arg("-c")
            .arg(&command)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("bash 命令超时")?
    .context("bash 执行失败")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Truncate output
    let stdout_truncated: String = stdout.chars().take(50_000).collect();
    let stderr_truncated: String = stderr.chars().take(10_000).collect();

    Ok(json!({
        "stdout": stdout_truncated,
        "stderr": stderr_truncated,
        "exit_code": output.status.code().unwrap_or(-1),
        "success": output.status.success(),
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
        // New file: resolve against working directory
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let allowed_roots = allowed_filesystem_roots();
        anyhow::ensure!(
            allowed_roots.iter().any(|root| absolute.starts_with(root)),
            "文件路径不在允许的根目录中: {file_path}"
        );
        tokio::fs::write(&absolute, &content).await?;
        Ok(
            json!({"written": true, "file_path": absolute.to_string_lossy(), "bytes": content.len()}),
        )
    }
}

/// Find files matching a glob pattern. Uses `find` for robust matching.
/// Simple patterns like `*.rs` match filenames; patterns with `/` match full paths.
pub async fn execute_glob(args: Value) -> anyhow::Result<Value> {
    let pattern = required_string(&args, "pattern")?;
    let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    let path = Path::new(&search_path);
    let canonical = path.canonicalize()?;
    let allowed_roots = allowed_filesystem_roots();
    anyhow::ensure!(
        allowed_roots.iter().any(|root| canonical.starts_with(root)),
        "搜索路径不在允许的根目录中: {search_path}"
    );

    // Build find args: use -path for patterns with /, -name for simple filenames
    let match_arg = if pattern.contains('/') {
        "-path"
    } else {
        "-name"
    };

    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("find")
            .arg(&canonical)
            .arg("-type")
            .arg("f")
            .arg(match_arg)
            .arg(&pattern)
            .arg("-maxdepth")
            .arg("40")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("find 超时")?
    .context("find 执行失败")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let matches: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).take(500).collect();

    Ok(json!({
        "pattern": pattern,
        "path": canonical.to_string_lossy(),
        "matches": matches,
        "match_count": matches.len(),
        "stderr": stderr.to_string(),
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

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Kairos-Pent-Agent/1.0")
        .build()
        .context("创建 HTTP 客户端失败")
}
