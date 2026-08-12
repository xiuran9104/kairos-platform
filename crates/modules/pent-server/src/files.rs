//! Local file management for pent artifacts — list/search/download/read/edit
//! files under `PENT_FILES_DIR` (default `storage/uploads`), with strict
//! path-traversal protection.

use std::path::{Component, Path, PathBuf};

use axum::{
    Json, Router,
    extract::{Multipart, Query, State},
    routing::{get, post},
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::{ApiResponse, PentState, security};

const MAX_CONTENT_BYTES: u64 = 1024 * 1024; // 1MB for content read/write
const MAX_UPLOAD_BYTES: u64 = 50 * 1024 * 1024; // 50MB upload limit
const SEARCH_MAX_DEPTH: usize = 8;
const SEARCH_MAX_RESULTS: usize = 500;

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "json", "log", "yaml", "yml", "toml", "xml", "csv", "js", "ts", "py", "rs",
    "go", "sh", "sql", "html", "css",
];

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/files", get(list_entries).delete(delete_entry))
        .route("/pent/files/download", get(download_file))
        .route(
            "/pent/files/content",
            get(read_content).put(write_content),
        )
        .route("/pent/files/rename", post(rename_entry))
        .route("/pent/files/mkdir", post(make_dir))
        .route("/pent/files/upload", post(upload_file))
}

fn root_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("PENT_FILES_DIR").unwrap_or_else(|_| "storage/uploads".to_string()),
    )
}

/// Normalize a user-supplied path to a POSIX-style path relative to the root
/// ("" = root). Rejects absolute paths, `..` traversal and illegal characters.
pub(crate) fn normalize_rel(path: &str) -> Result<String, AppError> {
    if path == "/" {
        return Ok(String::new());
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(AppError::bad_request("不允许绝对路径"));
    }
    let mut parts: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(AppError::bad_request("路径不允许包含 ..")),
            other if other.contains('\\') || other.contains('\0') => {
                return Err(AppError::bad_request("路径包含非法字符"));
            }
            other => parts.push(other),
        }
    }
    Ok(parts.join("/"))
}

/// Resolve a relative path against the root, double-checking the result never
/// escapes the root (belt-and-braces on top of `normalize_rel`).
fn resolve(root: &Path, rel: &str) -> PathBuf {
    let mut resolved = root.to_path_buf();
    for component in Path::new(rel).components() {
        if let Component::Normal(part) = component {
            resolved.push(part);
        }
    }
    resolved
}

fn valid_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

async fn ensure_root() -> Result<PathBuf, AppError> {
    let root = root_dir();
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|error| AppError::internal(format!("创建文件根目录失败：{error}")))?;
    Ok(root)
}

fn is_text_file(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        None => true, // extensionless files are treated as text
        Some(ext) => TEXT_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
    }
}

fn entry_json(root: &Path, path: &Path, name: String, metadata: &std::fs::Metadata) -> Value {
    let rel = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let modified_at = metadata
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
    json!({
        "name": name,
        "path": rel,
        "is_dir": metadata.is_dir(),
        "size": metadata.len(),
        "modified_at": modified_at,
    })
}

async fn list_entries(
    State(_s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(params.get("path").map(String::as_str).unwrap_or(""))?;
    let search = params
        .get("search")
        .map(String::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_lowercase);
    let page = params
        .get("page")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 200);

    let dir = resolve(&root, &rel);
    if !dir.is_dir() {
        return Err(AppError::not_found("目录不存在"));
    }

    let mut entries: Vec<Value> = Vec::new();
    match &search {
        None => {
            let mut read_dir = tokio::fs::read_dir(&dir)
                .await
                .map_err(|error| AppError::internal(format!("读取目录失败：{error}")))?;
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|error| AppError::internal(error.to_string()))?
            {
                let metadata = entry
                    .metadata()
                    .await
                    .map_err(|error| AppError::internal(error.to_string()))?;
                entries.push(entry_json(
                    &root,
                    &entry.path(),
                    entry.file_name().to_string_lossy().into_owned(),
                    &metadata,
                ));
            }
            // Directories first, then by name.
            entries.sort_by(|a, b| {
                let dir_a = a["is_dir"].as_bool().unwrap_or(false);
                let dir_b = b["is_dir"].as_bool().unwrap_or(false);
                dir_b.cmp(&dir_a).then_with(|| {
                    a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
                })
            });
        }
        Some(keyword) => {
            // Iterative DFS with depth/result caps.
            let mut stack = vec![(dir.clone(), 0usize)];
            while let Some((current, depth)) = stack.pop() {
                if entries.len() >= SEARCH_MAX_RESULTS {
                    break;
                }
                if depth > SEARCH_MAX_DEPTH {
                    continue;
                }
                let Ok(mut read_dir) = tokio::fs::read_dir(&current).await else {
                    continue;
                };
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let Ok(metadata) = entry.metadata().await else {
                        continue;
                    };
                    if metadata.is_dir() {
                        stack.push((entry.path(), depth + 1));
                    }
                    if name.to_lowercase().contains(keyword) {
                        entries.push(entry_json(&root, &entry.path(), name, &metadata));
                        if entries.len() >= SEARCH_MAX_RESULTS {
                            break;
                        }
                    }
                }
            }
            entries.sort_by(|a, b| {
                a["path"].as_str().unwrap_or("").cmp(b["path"].as_str().unwrap_or(""))
            });
        }
    }

    let total = entries.len() as i64;
    let offset = ((page - 1) * page_size) as usize;
    let page_entries: Vec<Value> = entries
        .into_iter()
        .skip(offset)
        .take(page_size as usize)
        .collect();
    Ok(Json(ApiResponse::new(json!({
        "path": rel,
        "entries": page_entries,
        "total": total,
        "page": page,
        "page_size": page_size,
    }))))
}

async fn download_file(
    State(_s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<axum::response::Response, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(
        params
            .get("path")
            .map(String::as_str)
            .ok_or_else(|| AppError::bad_request("path 不能为空"))?,
    )?;
    let path = resolve(&root, &rel);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::not_found("文件不存在"))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("只能下载文件"));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| AppError::internal(format!("读取文件失败：{error}")))?;
    let filename = rel.rsplit('/').next().unwrap_or("download.bin");
    Ok(axum::response::Response::builder()
        .header("content-type", "application/octet-stream")
        .header(
            "content-disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .header("content-length", bytes.len())
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

async fn read_content(
    State(_s): State<PentState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(
        params
            .get("path")
            .map(String::as_str)
            .ok_or_else(|| AppError::bad_request("path 不能为空"))?,
    )?;
    let path = resolve(&root, &rel);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::not_found("文件不存在"))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("只能读取文件内容"));
    }
    if !is_text_file(&path) {
        return Err(AppError::bad_request("仅支持文本文件"));
    }
    if metadata.len() > MAX_CONTENT_BYTES {
        return Err(AppError::bad_request("文件超过 1MB，无法在线查看"));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| AppError::internal(format!("读取文件失败：{error}")))?;
    if bytes.contains(&0) {
        return Err(AppError::bad_request("二进制文件不支持在线查看"));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| AppError::bad_request("文件不是有效的 UTF-8 文本"))?;
    Ok(Json(ApiResponse::new(json!({
        "path": rel,
        "content": content,
    }))))
}

async fn write_content(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(
        body.get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::bad_request("path 不能为空"))?,
    )?;
    let content = body
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("content 不能为空"))?;
    if content.len() as u64 > MAX_CONTENT_BYTES {
        return Err(AppError::bad_request("内容超过 1MB，无法保存"));
    }
    let path = resolve(&root, &rel);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::not_found("文件不存在，只能编辑已有文件"))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("只能编辑文件"));
    }
    if !is_text_file(&path) {
        return Err(AppError::bad_request("仅支持文本文件"));
    }
    tokio::fs::write(&path, content)
        .await
        .map_err(|error| AppError::internal(format!("保存文件失败：{error}")))?;
    Ok(Json(ApiResponse::new(json!({"saved": true}))))
}

async fn rename_entry(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(
        body.get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::bad_request("path 不能为空"))?,
    )?;
    if rel.is_empty() {
        return Err(AppError::bad_request("根目录不能重命名"));
    }
    let new_name = body
        .get("new_name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("new_name 不能为空"))?;
    if !valid_entry_name(new_name) {
        return Err(AppError::bad_request("new_name 不能包含 / 或非法字符"));
    }
    let from = resolve(&root, &rel);
    tokio::fs::metadata(&from)
        .await
        .map_err(|_| AppError::not_found("文件或目录不存在"))?;
    let to = from
        .parent()
        .ok_or_else(|| AppError::bad_request("路径无效"))?
        .join(new_name);
    if to.exists() {
        return Err(AppError::bad_request("目标名称已存在"));
    }
    tokio::fs::rename(&from, &to)
        .await
        .map_err(|error| AppError::internal(format!("重命名失败：{error}")))?;
    Ok(Json(ApiResponse::new(json!({"renamed": true}))))
}

async fn make_dir(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(body.get("path").and_then(Value::as_str).unwrap_or(""))?;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("name 不能为空"))?;
    if !valid_entry_name(name) {
        return Err(AppError::bad_request("name 不能包含 / 或非法字符"));
    }
    let parent = resolve(&root, &rel);
    if !parent.is_dir() {
        return Err(AppError::not_found("目录不存在"));
    }
    let target = parent.join(name);
    if target.exists() {
        return Err(AppError::bad_request("同名文件或目录已存在"));
    }
    tokio::fs::create_dir(&target)
        .await
        .map_err(|error| AppError::internal(format!("创建目录失败：{error}")))?;
    Ok(Json(ApiResponse::new(json!({"created": true}))))
}

async fn delete_entry(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;
    let rel = normalize_rel(
        body.get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::bad_request("path 不能为空"))?,
    )?;
    if rel.is_empty() {
        return Err(AppError::bad_request("根目录不能删除"));
    }
    let path = resolve(&root, &rel);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::not_found("文件或目录不存在"))?;
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(&path)
            .await
            .map_err(|error| AppError::internal(format!("删除目录失败：{error}")))?;
    } else {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|error| AppError::internal(format!("删除文件失败：{error}")))?;
    }
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}

async fn upload_file(
    State(_s): State<PentState>,
    user: CurrentUser,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::tenant_id(&user)?;
    let root = ensure_root().await?;

    // Fields may arrive in any order, so stream the file to a temporary name
    // first, then move it once the optional `path` field is known.
    let mut target_dir = String::new();
    let mut file_name: Option<String> = None;
    let mut temp_path: Option<PathBuf> = None;
    let mut written: u64 = 0;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::bad_request(format!("multipart 解析失败：{error}")))?
    {
        match field.name() {
            Some("path") => {
                let value = field
                    .text()
                    .await
                    .map_err(|error| AppError::bad_request(error.to_string()))?;
                target_dir = normalize_rel(&value)?;
            }
            Some("file") => {
                let name = field
                    .file_name()
                    .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n).to_string())
                    .filter(|n| valid_entry_name(n))
                    .ok_or_else(|| AppError::bad_request("上传文件名无效"))?;
                let temp = root.join(format!(".upload-{}", uuid::Uuid::new_v4()));
                let mut file = tokio::fs::File::create(&temp)
                    .await
                    .map_err(|error| AppError::internal(format!("创建临时文件失败：{error}")))?;
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            written += chunk.len() as u64;
                            if written > MAX_UPLOAD_BYTES {
                                drop(file);
                                let _ = tokio::fs::remove_file(&temp).await;
                                return Err(AppError::bad_request("上传文件超过 50MB"));
                            }
                            use tokio::io::AsyncWriteExt;
                            file.write_all(&chunk)
                                .await
                                .map_err(|error| {
                                    AppError::internal(format!("写入文件失败：{error}"))
                                })?;
                        }
                        Ok(None) => break,
                        Err(error) => {
                            drop(file);
                            let _ = tokio::fs::remove_file(&temp).await;
                            return Err(AppError::bad_request(format!("上传失败：{error}")));
                        }
                    }
                }
                file_name = Some(name);
                temp_path = Some(temp);
            }
            _ => {}
        }
    }

    let (Some(name), Some(temp)) = (file_name, temp_path) else {
        return Err(AppError::bad_request("缺少 file 字段"));
    };
    let dir = resolve(&root, &target_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| AppError::internal(format!("创建目录失败：{error}")))?;
    let target = dir.join(&name);
    if target.exists() {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(AppError::bad_request("同名文件已存在"));
    }
    tokio::fs::rename(&temp, &target)
        .await
        .map_err(|error| AppError::internal(format!("保存上传文件失败：{error}")))?;
    let rel = if target_dir.is_empty() {
        name
    } else {
        format!("{target_dir}/{name}")
    };
    Ok(Json(ApiResponse::new(json!({
        "path": rel,
        "size": written,
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_accepts_root_and_plain_paths() {
        assert_eq!(normalize_rel("").unwrap(), "");
        assert_eq!(normalize_rel("/").unwrap(), "");
        assert_eq!(normalize_rel("a/b/c").unwrap(), "a/b/c");
        assert_eq!(normalize_rel("a/./b/").unwrap(), "a/b");
        assert_eq!(normalize_rel("./report.md").unwrap(), "report.md");
    }

    #[test]
    fn normalize_rejects_traversal() {
        assert!(normalize_rel("..").is_err());
        assert!(normalize_rel("../etc/passwd").is_err());
        assert!(normalize_rel("a/../../b").is_err());
        assert!(normalize_rel("a/b/..").is_err());
    }

    #[test]
    fn normalize_rejects_absolute_and_illegal() {
        assert!(normalize_rel("/etc/passwd").is_err());
        assert!(normalize_rel("C:\\Windows").is_err());
        assert!(normalize_rel("a\\b").is_err());
    }

    #[test]
    fn entry_name_validation() {
        assert!(valid_entry_name("report.md"));
        assert!(!valid_entry_name(""));
        assert!(!valid_entry_name("a/b"));
        assert!(!valid_entry_name(".."));
        assert!(!valid_entry_name("."));
        assert!(!valid_entry_name("a\\b"));
    }
}
