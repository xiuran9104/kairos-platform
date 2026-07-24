use crate::{ToonState, ai_client, shared::require, toonflow_image_edit_prompt};
use axum::{Json, extract::State};
use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbaImage, imageops};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};

async fn normalize_image_references(references: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(references.len());
    for reference in references {
        if reference.starts_with("data:")
            || reference.starts_with("http://")
            || reference.starts_with("https://")
        {
            normalized.push(reference);
        } else if reference.starts_with("/toonflow/assets/files/") {
            normalized.push(crate::toonflow_storage::image_data_url(&reference).await?);
        } else {
            return Err(format!("不支持的参考图地址：{reference}"));
        }
    }
    Ok(normalized)
}

fn validate_storyboard_prompt(prompt: &str, reference_count: usize) -> Result<(), String> {
    if prompt.trim().is_empty() {
        return Err("分镜图片提示词为空，请先按首位帧模式重新写入分镜面板".to_string());
    }
    for index in 1..=reference_count {
        let marker = format!("@图{index}");
        if !prompt.contains(&marker) {
            return Err(format!(
                "分镜图片提示词缺少参考资产绑定 {marker}，请重新生成分镜面板"
            ));
        }
    }
    Ok(())
}

fn storyboard_image_size(quality: &str, ratio: &str) -> String {
    let long_edge = match quality.trim().to_ascii_uppercase().as_str() {
        "1K" => 1280.0,
        "4K" => 3840.0,
        _ => 2560.0,
    };
    let Some((width, height)) = ratio.split_once(':').and_then(|(width, height)| {
        Some((width.parse::<f64>().ok()?, height.parse::<f64>().ok()?))
    }) else {
        return quality.to_string();
    };
    if width <= 0.0 || height <= 0.0 {
        return quality.to_string();
    }
    let (pixel_width, pixel_height) = if width >= height {
        (long_edge, long_edge * height / width)
    } else {
        (long_edge * width / height, long_edge)
    };
    let align = |value: f64| ((value / 32.0).round().max(1.0) * 32.0) as u32;
    format!("{}x{}", align(pixel_width), align(pixel_height))
}

#[derive(Deserialize)]
pub struct FlowId {
    id: i64,
}
pub async fn get_flow(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<FlowId>,
) -> Result<Json<ApiResponse<Option<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let row: Option<(i64, Value)> =
        sqlx::query_as("SELECT id,flow_data FROM toonflow.image_flows WHERE id=$1")
            .bind(req.id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get image flow"))?;
    Ok(Json(ApiResponse::new(row.map(|r| {
        let mut value = r.1;
        value["id"] = json!(r.0);
        value
    }))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFlow {
    asset_id: Option<i64>,
    edges: Value,
    nodes: Value,
}
pub async fn save_flow(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<SaveFlow>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let id = chrono::Utc::now().timestamp_millis();
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to begin image flow transaction"))?;
    sqlx::query("INSERT INTO toonflow.image_flows(id,flow_data)VALUES($1,$2)")
        .bind(id)
        .bind(json!({"edges":req.edges,"nodes":req.nodes}))
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to save image flow"))?;
    if let Some(asset_id) = req.asset_id {
        sqlx::query("UPDATE toonflow.assets SET flow_id=$2 WHERE id=$1")
            .bind(asset_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to bind image flow to asset"))?;
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to commit image flow"))?;
    Ok(Json(ApiResponse::new(json!({"id":id}))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFlow {
    flow_id: i64,
    edges: Value,
    nodes: Value,
}
pub async fn update_flow(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<UpdateFlow>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    sqlx::query("UPDATE toonflow.image_flows SET flow_data=$2 WHERE id=$1")
        .bind(req.flow_id)
        .bind(json!({"edges":req.edges,"nodes":req.nodes}))
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update image flow"))?;
    Ok(Json(ApiResponse::new(())))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowImage {
    model: String,
    references: Option<Vec<String>>,
    quality: String,
    ratio: String,
    prompt: String,
    project_id: i64,
    #[serde(default)]
    target_type: String,
}
pub async fn generate_flow_image(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<FlowImage>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let references = normalize_image_references(req.references.unwrap_or_default())
        .await
        .map_err(AppError::bad_request)?;
    let prompt = toonflow_image_edit_prompt::build(
        toonflow_image_edit_prompt::ImageEditTarget::parse(&req.target_type),
        &req.prompt,
        &req.ratio,
        references.len(),
    );
    let url = ai_client::image_with_references(
        &state.pool,
        &req.model,
        &prompt,
        &req.quality,
        references,
    )
    .await
    .map_err(AppError::bad_request)?;
    let now = chrono::Utc::now().timestamp_millis();
    let _=sqlx::query("INSERT INTO toonflow.tasks(id,project_id,task_class,related_objects,model,description,state,start_time)VALUES($1,$2,'工作流图片生成',$3,$4,'工作流图片生成','success',$1)").bind(now).bind(req.project_id).bind(json!({"prompt":req.prompt}).to_string()).bind(req.model).execute(&state.pool).await;
    Ok(Json(ApiResponse::new(json!({"url":url}))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectId {
    project_id: i64,
}
pub async fn default_model(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<ProjectId>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:read")?;
    let row: Option<(Option<i64>, String)> =
        sqlx::query_as("SELECT image_model,image_quality FROM toonflow.projects WHERE id=$1")
            .bind(req.project_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get image model"))?;
    Ok(Json(ApiResponse::new(
        row.map(|r| json!({"imageModel":r.0,"imageQuality":r.1}))
            .unwrap_or(Value::Null),
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardGenerate {
    storyboard_ids: Vec<i64>,
    project_id: i64,
    script_id: i64,
    concurrent_count: Option<usize>,
    compulsory: Option<bool>,
}

pub async fn schedule_storyboard_generation(
    pool: &sqlx::PgPool,
    project_id: i64,
    script_id: i64,
    storyboard_ids: &[i64],
    concurrent_count: usize,
    compulsory: bool,
) -> Result<Vec<Value>, AppError> {
    if storyboard_ids.is_empty() {
        return Err(AppError::bad_request("storyboardIds不能为空"));
    }
    crate::toonflow_storyboard_asset_validation::reject_base_roles_for_storyboards(
        pool,
        storyboard_ids,
    )
    .await?;
    let rows=sqlx::query_as::<_,(i64,String,i32)>("SELECT id,prompt,should_generate_image FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 AND id=ANY($3)").bind(project_id).bind(script_id).bind(storyboard_ids).fetch_all(pool).await.map_err(|_|AppError::internal("failed to get storyboards"))?;
    if rows.is_empty() {
        return Err(AppError::not_found("未查到分镜数据"));
    }
    let ids = rows
        .iter()
        .filter(|row| compulsory || row.2 != 0)
        .map(|row| row.0)
        .collect::<Vec<_>>();
    sqlx::query("UPDATE toonflow.storyboards SET state='生成中',reason=NULL WHERE id=ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await
        .map_err(|_| AppError::internal("failed to start storyboards"))?;
    let response=rows.iter().map(|row|json!({"id":row.0,"prompt":row.1,"state":if ids.contains(&row.0){"生成中"}else{"未生成"},"shouldGenerateImage":row.2})).collect();
    let pool = pool.clone();
    tokio::spawn(async move {
        let setting: Option<(Option<i64>, String, String)> = sqlx::query_as(
            "SELECT image_model,image_quality,video_ratio FROM toonflow.projects WHERE id=$1",
        )
        .bind(project_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
        let Some((Some(model), quality, ratio)) = setting else {
            return;
        };
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrent_count.clamp(1, 10)));
        for (id, prompt, should) in rows {
            if !compulsory && should == 0 {
                continue;
            }
            let permit = sem.clone().acquire_owned().await;
            let pool = pool.clone();
            let model = model.to_string();
            let quality = quality.clone();
            let ratio = ratio.clone();
            tokio::spawn(async move {
                if permit.is_ok() {
                    let reference_paths =
                        crate::toonflow_asset_context::load_storyboard_asset_references(
                            &pool, project_id, script_id, id,
                        )
                        .await
                        .unwrap_or_default();
                    let references = match normalize_image_references(reference_paths).await {
                        Ok(references) => references,
                        Err(reason) => {
                            let reason = format!("分镜参考资产读取失败：{reason}");
                            let _ = sqlx::query(
                                "UPDATE toonflow.storyboards SET state='生成失败',reason=$2 WHERE id=$1",
                            )
                            .bind(id)
                            .bind(reason)
                            .execute(&pool)
                            .await;
                            return;
                        }
                    };
                    if let Err(reason) = validate_storyboard_prompt(&prompt, references.len()) {
                        let _ = sqlx::query(
                            "UPDATE toonflow.storyboards SET state='生成失败',reason=$2 WHERE id=$1",
                        )
                        .bind(id)
                        .bind(reason)
                        .execute(&pool)
                        .await;
                        return;
                    }
                    match ai_client::image_with_references(
                        &pool,
                        &model,
                        &prompt,
                        &storyboard_image_size(&quality, &ratio),
                        references,
                    )
                    .await
                    {
                        Ok(url) => {
                            let _=sqlx::query("UPDATE toonflow.storyboards SET file_path=$2,state='已完成',reason=NULL WHERE id=$1").bind(id).bind(url).execute(&pool).await;
                        }
                        Err(reason) => {
                            let _=sqlx::query("UPDATE toonflow.storyboards SET state='生成失败',reason=$2 WHERE id=$1").bind(id).bind(reason).execute(&pool).await;
                        }
                    }
                }
            });
        }
    });
    Ok(response)
}

#[cfg(test)]
mod prompt_tests {
    use super::{storyboard_image_size, validate_storyboard_prompt};

    #[test]
    fn accepts_ordered_reference_markers() {
        assert!(
            validate_storyboard_prompt("@图1 为角色，@图2 为场景，【画面】二人对视", 2).is_ok()
        );
    }

    #[test]
    fn rejects_missing_reference_markers() {
        let error = validate_storyboard_prompt("@图1 为角色，【画面】二人对视", 2)
            .expect_err("missing @图2 must be rejected");
        assert!(error.contains("@图2"));
    }

    #[test]
    fn rejects_empty_prompt() {
        assert!(validate_storyboard_prompt("  ", 0).is_err());
    }

    #[test]
    fn converts_project_ratio_to_provider_dimensions() {
        assert_eq!(storyboard_image_size("2K", "16:9"), "2560x1440");
        assert_eq!(storyboard_image_size("2K", "9:16"), "1440x2560");
        assert_eq!(storyboard_image_size("2K", "1:1"), "2560x2560");
    }
}

pub async fn generate_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<StoryboardGenerate>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:update")?;
    let response = schedule_storyboard_generation(
        &state.pool,
        req.project_id,
        req.script_id,
        &req.storyboard_ids,
        req.concurrent_count.unwrap_or(5),
        req.compulsory.unwrap_or(false),
    )
    .await?;
    Ok(Json(ApiResponse::new(response)))
}

#[derive(Deserialize)]
pub struct Ids {
    ids: Vec<i64>,
}
pub async fn poll_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Ids>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows=sqlx::query_as::<_,(i64,String,Option<String>,Option<String>,String)>("SELECT id,coalesce(state,''),reason,file_path,prompt FROM toonflow.storyboards WHERE id=ANY($1) AND state<>'生成中'").bind(req.ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to poll storyboards"))?;
    Ok(Json(ApiResponse::new(rows.into_iter().map(|r|json!({"id":r.0,"state":r.1,"reason":r.2,"filePath":r.3,"src":r.3,"prompt":r.4})).collect())))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardUrl {
    id: i64,
    url: String,
    flow_id: i64,
}
pub async fn update_storyboard_url(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<StoryboardUrl>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:update")?;
    sqlx::query("UPDATE toonflow.storyboards SET file_path=$2,flow_id=$3,state='已完成',should_generate_image=$4 WHERE id=$1").bind(req.id).bind(&req.url).bind(req.flow_id).bind(if req.url.is_empty(){0}else{1}).execute(&state.pool).await.map_err(|_|AppError::internal("failed to update storyboard image"))?;
    Ok(Json(ApiResponse::new(json!({"message":"更新分镜成功"}))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteStoryboards {
    pub(crate) ids: Vec<i64>,
    pub(crate) project_id: i64,
}
pub async fn delete_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<DeleteStoryboards>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:delete")?;
    if req.ids.is_empty() {
        return Err(AppError::bad_request("请先选择分镜"));
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to begin transaction"))?;
    let rows: Vec<(i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT id,track_id,flow_id FROM toonflow.storyboards WHERE id=ANY($1) AND project_id=$2",
    )
    .bind(&req.ids)
    .bind(req.project_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to load storyboards"))?;
    if rows.is_empty() {
        return Err(AppError::not_found("当前选择分镜不存在"));
    }
    let storyboard_ids = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let track_ids = rows
        .iter()
        .filter_map(|row| row.1)
        .collect::<std::collections::BTreeSet<_>>();
    let flow_ids = rows.iter().filter_map(|row| row.2).collect::<Vec<_>>();
    sqlx::query("DELETE FROM toonflow.assets_storyboards WHERE storyboard_id=ANY($1)")
        .bind(&storyboard_ids)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to delete storyboard assets"))?;
    sqlx::query("DELETE FROM toonflow.storyboards WHERE id=ANY($1) AND project_id=$2")
        .bind(&storyboard_ids)
        .bind(req.project_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to delete storyboards"))?;
    if !flow_ids.is_empty() {
        sqlx::query("DELETE FROM toonflow.image_flows WHERE id=ANY($1)")
            .bind(&flow_ids)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to delete storyboard image flows"))?;
    }
    for track_id in track_ids {
        let remaining: i64 =
            sqlx::query_scalar("SELECT count(*) FROM toonflow.storyboards WHERE track_id=$1")
                .bind(track_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to inspect storyboard track"))?;
        if remaining == 0 {
            sqlx::query("DELETE FROM toonflow.video_tracks WHERE id=$1")
                .bind(track_id)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to delete storyboard track"))?;
        } else {
            sqlx::query("UPDATE toonflow.video_tracks SET duration=(SELECT coalesce(sum(CASE WHEN duration ~ '^[0-9]+$' THEN duration::integer ELSE 0 END),0)::integer FROM toonflow.storyboards WHERE track_id=$1) WHERE id=$1")
                .bind(track_id)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to update storyboard track"))?;
        }
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to commit transaction"))?;
    Ok(Json(ApiResponse::new(json!({"message":"视频删除成功"}))))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    #[serde(alias = "ids")]
    storyboard_ids: Vec<i64>,
}

async fn preview_png(pool: &sqlx::PgPool, ids: &[i64]) -> Result<Option<Vec<u8>>, AppError> {
    let rows = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT id,file_path FROM toonflow.storyboards WHERE id=ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to get storyboard images"))?;
    let paths = rows
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    let mut images = Vec::new();
    let mut original_images = Vec::new();
    for id in ids {
        let Some(path) = paths.get(id).and_then(Clone::clone) else {
            continue;
        };
        let bytes = if let Some(encoded) = path.split_once(";base64,").map(|v| v.1) {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| AppError::bad_request("invalid image data"))?
        } else {
            reqwest::get(path)
                .await
                .map_err(|_| AppError::bad_request("failed to fetch storyboard image"))?
                .bytes()
                .await
                .map_err(|_| AppError::bad_request("failed to read storyboard image"))?
                .to_vec()
        };
        if looks_like_image(&bytes) {
            original_images.push(bytes.clone());
        }
        if let Ok(image) = image::load_from_memory(&bytes) {
            images.push(image);
        }
    }
    if images.is_empty() {
        if ids.len() == 1 {
            return Ok(original_images.into_iter().next());
        }
        return Ok(None);
    }
    let thumb = 256u32;
    let cols = images.len().min(5) as u32;
    let row_count = images.len().div_ceil(cols as usize) as u32;
    let mut canvas = RgbaImage::from_pixel(
        cols * thumb,
        row_count * thumb,
        image::Rgba([255, 255, 255, 255]),
    );
    for (index, image) in images.into_iter().enumerate() {
        let resized = image.resize(thumb, thumb, imageops::FilterType::Lanczos3);
        let x = index as u32 % cols * thumb + (thumb - resized.width()) / 2;
        let y = index as u32 / cols * thumb + (thumb - resized.height()) / 2;
        imageops::overlay(&mut canvas, &resized, x.into(), y.into());
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|_| AppError::internal("failed to encode preview"))?;
    Ok(Some(cursor.into_inner()))
}

fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
}

pub async fn preview_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<ApiResponse<Option<String>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let png = preview_png(&state.pool, &req.storyboard_ids).await?;
    Ok(Json(ApiResponse::new(png.map(|data| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(data)
        )
    }))))
}

pub async fn download_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<PreviewRequest>,
) -> Result<axum::response::Response, AppError> {
    require(&user, "toon:scene:read")?;
    let png = preview_png(&state.pool, &req.storyboard_ids)
        .await?
        .ok_or_else(|| AppError::not_found("没有可下载的分镜图片"))?;
    axum::response::Response::builder()
        .header("content-type", "image/png")
        .header(
            "content-disposition",
            "attachment; filename=storyboard-preview.png",
        )
        .body(axum::body::Body::from(png))
        .map_err(|_| AppError::internal("failed to build download"))
}

#[cfg(test)]
mod tests {
    use super::normalize_image_references;

    #[tokio::test]
    async fn keeps_provider_usable_reference_urls() {
        let references = vec![
            "https://cdn.example.com/reference.jpg".to_string(),
            "data:image/png;base64,iVBORw0KGgo=".to_string(),
        ];
        assert_eq!(
            normalize_image_references(references.clone())
                .await
                .unwrap(),
            references
        );
    }

    #[tokio::test]
    async fn rejects_unknown_relative_reference_paths() {
        let error = normalize_image_references(vec!["/unknown/image.jpg".to_string()])
            .await
            .unwrap_err();
        assert!(error.contains("不支持的参考图地址"));
    }
}
