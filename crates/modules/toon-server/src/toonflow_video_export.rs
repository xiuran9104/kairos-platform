use crate::{ToonState, shared::require};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    project_id: i64,
    script_id: i64,
}

fn storage_dir() -> PathBuf {
    std::env::var_os("INFRA_UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("storage/uploads"))
}

async fn ffmpeg_available() -> bool {
    tokio::task::spawn_blocking(|| {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
    })
    .await
    .unwrap_or(false)
}

async fn materialize_video(
    source: &str,
    directory: &Path,
    index: usize,
) -> Result<PathBuf, String> {
    let destination = directory.join(format!("{index:04}.mp4"));
    if let Some(relative) = source.strip_prefix("/upload/") {
        tokio::fs::copy(storage_dir().join(relative), &destination)
            .await
            .map_err(|error| format!("无法读取本地视频：{error}"))?;
    } else if source.starts_with("http://") || source.starts_with("https://") {
        let bytes = reqwest::get(source)
            .await
            .map_err(|error| format!("下载视频失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("下载视频失败：{error}"))?
            .bytes()
            .await
            .map_err(|error| format!("读取视频失败：{error}"))?;
        tokio::fs::write(&destination, bytes)
            .await
            .map_err(|error| format!("保存视频失败：{error}"))?;
    } else {
        return Err(format!("不支持的视频地址：{source}"));
    }
    tokio::fs::canonicalize(destination)
        .await
        .map_err(|error| format!("解析视频路径失败：{error}"))
}

async fn run_export(
    pool: sqlx::PgPool,
    project_id: i64,
    script_id: i64,
    task_id: i64,
) -> Result<String, String> {
    let sources:Vec<String>=sqlx::query_scalar("SELECT v.file_path FROM toonflow.video_tracks t JOIN toonflow.videos v ON v.id=t.video_id WHERE t.project_id=$1 AND t.script_id=$2 AND v.state='生成成功' AND coalesce(v.file_path,'')<>'' ORDER BY t.sort_order,t.id").bind(project_id).bind(script_id).fetch_all(&pool).await.map_err(|error|error.to_string())?;
    if sources.is_empty() {
        return Err("请先为每条轨道选择已生成的视频".into());
    }
    let work_dir = storage_dir().join(format!("toonflow/{project_id}/exports/work-{task_id}"));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        files.push(materialize_video(source, &work_dir, index).await?);
    }
    let list_path = work_dir.join("concat.txt");
    let list = files
        .iter()
        .map(|path| format!("file '{}'", path.to_string_lossy().replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join("\n");
    tokio::fs::write(&list_path, list)
        .await
        .map_err(|error| error.to_string())?;
    let relative = format!("toonflow/{project_id}/exports/{script_id}_{task_id}.mp4");
    let output_path = storage_dir().join(&relative);
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("ffmpeg")
            .args(["-y", "-f", "concat", "-safe", "0", "-i"])
            .arg(&list_path)
            .args(["-c:v", "libx264", "-c:a", "aac", "-movflags", "+faststart"])
            .arg(&output_path)
            .output()
    })
    .await
    .map_err(|error| format!("FFmpeg 任务异常：{error}"))?
    .map_err(|error| format!("启动 FFmpeg 失败：{error}"))?;
    let _ = tokio::fs::remove_dir_all(&work_dir).await;
    if !output.status.success() {
        return Err(format!(
            "成片导出失败：{}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .last()
                .unwrap_or("FFmpeg 执行失败")
        ));
    }
    Ok(format!("/upload/{relative}"))
}

pub async fn export(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ExportRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:update")?;
    if !ffmpeg_available().await {
        return Err(AppError::bad_request(
            "服务器未安装 FFmpeg，安装后即可使用成片导出",
        ));
    }
    let tm = crate::toonflow_tasks::TaskManager::new(state.pool.clone());
    let task_id = tm.create("videoExport", Some(request.project_id), "ffmpeg", "合并选中视频为最终成片", &json!({"scriptId":request.script_id}).to_string()).await.map_err(|e| AppError::internal(e.to_string()))?;
    tm.set_running(task_id).await.map_err(|e| AppError::internal(e.to_string()))?;
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let tm = crate::toonflow_tasks::TaskManager::new(pool.clone());
        match run_export(pool.clone(), request.project_id, request.script_id, task_id).await {
            Ok(url) => {
                let _ = tm.set_succeeded(task_id, Some(&json!({"scriptId":request.script_id,"url":url}).to_string()), None).await;
            }
            Err(reason) => {
                let _ = tm.set_failed(task_id, &reason).await;
            }
        }
    });
    Ok(Json(ApiResponse::new(
        json!({"taskId":task_id,"state":"running"}),
    )))
}
