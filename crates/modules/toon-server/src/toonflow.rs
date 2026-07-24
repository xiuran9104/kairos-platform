use axum::{
    Json,
    extract::{Path, State},
};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Row};

use crate::{
    ToonState,
    shared::{affected, current_user_id, require},
};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn next_id(offset: i64) -> i64 {
    now_ms() + offset
}

async fn ensure_project(pool: &PgPool, project_id: i64) -> Result<(), AppError> {
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM toonflow.projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::internal("failed to check project"))?;
    exists
        .map(|_| ())
        .ok_or_else(|| AppError::not_found("project not found"))
}
async fn validate_project_models(
    pool: &PgPool,
    image: Option<i64>,
    video: Option<i64>,
) -> Result<(), AppError> {
    for (id, kind) in [(image, "image"), (video, "video")] {
        if let Some(id) = id {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ai.model_configs WHERE id=$1 AND type=$2 AND status=0)").bind(id).bind(kind).fetch_one(pool).await.map_err(|_|AppError::internal("failed to validate AI model"))?;
            if !valid {
                return Err(AppError::bad_request(format!("请选择启用的{kind}模型")));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ToonflowProject {
    pub id: i64,
    pub project_type: String,
    pub image_model: Option<i64>,
    pub image_quality: String,
    pub video_model: Option<i64>,
    pub name: String,
    pub intro: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub art_style: String,
    pub director_manual: String,
    pub mode: String,
    pub video_ratio: String,
    pub create_time: i64,
    pub update_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProjectRequest {
    pub id: Option<i64>,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub image_model: Option<i64>,
    #[serde(default)]
    pub image_quality: String,
    #[serde(default)]
    pub video_model: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub intro: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub art_style: String,
    #[serde(default)]
    pub director_manual: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub video_ratio: String,
}

#[derive(Debug, Deserialize)]
pub struct IdRequest {
    pub id: i64,
}

pub async fn list_projects(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<ToonflowProject>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, ToonflowProject>(
        r#"SELECT id, project_type, image_model, image_quality, video_model, name, intro,
                  type as type_, art_style, director_manual, mode, video_ratio, create_time, update_time
           FROM toonflow.projects ORDER BY create_time DESC"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list toonflow projects"))?;
    Ok(Json(ApiResponse::new(rows)))
}

pub async fn create_project(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveProjectRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:create")?;
    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("project name is required"));
    }
    validate_project_models(&state.pool, request.image_model, request.video_model).await?;
    let id = request.id.unwrap_or_else(|| next_id(0));
    let time = now_ms();
    sqlx::query(
        r#"INSERT INTO toonflow.projects
           (id, project_type, image_model, image_quality, video_model, name, intro, type,
            art_style, director_manual, mode, video_ratio, user_id, create_time, update_time)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14)"#,
    )
    .bind(id)
    .bind(request.project_type)
    .bind(request.image_model)
    .bind(request.image_quality)
    .bind(request.video_model)
    .bind(request.name.trim())
    .bind(request.intro)
    .bind(request.r#type)
    .bind(request.art_style)
    .bind(request.director_manual)
    .bind(request.mode)
    .bind(request.video_ratio)
    .bind(current_user_id(&user)?)
    .bind(time)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to create toonflow project"))?;
    Ok(Json(ApiResponse::with_message(
        json!({ "id": id }),
        "新增项目成功",
    )))
}

pub async fn update_project(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveProjectRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let id = request
        .id
        .ok_or_else(|| AppError::bad_request("project id is required"))?;
    validate_project_models(&state.pool, request.image_model, request.video_model).await?;
    let result = sqlx::query(
        r#"UPDATE toonflow.projects
           SET project_type=$2, image_model=$3, image_quality=$4, video_model=$5, name=$6,
               intro=$7, type=$8, art_style=$9, director_manual=$10, mode=$11,
               video_ratio=$12, update_time=$13
           WHERE id=$1"#,
    )
    .bind(id)
    .bind(request.project_type)
    .bind(request.image_model)
    .bind(request.image_quality)
    .bind(request.video_model)
    .bind(request.name.trim())
    .bind(request.intro)
    .bind(request.r#type)
    .bind(request.art_style)
    .bind(request.director_manual)
    .bind(request.mode)
    .bind(request.video_ratio)
    .bind(now_ms())
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to update toonflow project"))?;
    affected(result.rows_affected(), "project")?;
    Ok(Json(ApiResponse::with_message((), "编辑项目成功")))
}

pub async fn delete_project(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<IdRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:delete")?;
    let result = sqlx::query("DELETE FROM toonflow.projects WHERE id = $1")
        .bind(request.id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete toonflow project"))?;
    affected(result.rows_affected(), "project")?;
    Ok(Json(ApiResponse::with_message((), "删除项目成功")))
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct NovelChapter {
    pub id: i64,
    #[serde(rename = "index")]
    pub chapter_index: i32,
    pub reel: String,
    pub chapter: String,
    pub chapter_data: String,
    pub project_id: i64,
    pub event_state: i32,
    pub event: Option<String>,
    pub error_reason: Option<String>,
    pub create_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NovelItemRequest {
    #[serde(default)]
    pub index: i32,
    #[serde(default)]
    pub reel: String,
    pub chapter: String,
    pub chapter_data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNovelRequest {
    pub project_id: i64,
    pub data: Vec<NovelItemRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListNovelRequest {
    pub project_id: i64,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub search: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct PageData<T: Serialize> {
    pub data: Vec<T>,
    pub total: i64,
}

pub async fn add_novel(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<AddNovelRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    ensure_project(&state.pool, request.project_id).await?;
    let last: Option<(i32,)> = sqlx::query_as(
        "SELECT chapter_index FROM toonflow.novels WHERE project_id=$1 ORDER BY chapter_index DESC LIMIT 1",
    )
    .bind(request.project_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to read novel chapters"))?;
    let mut chapter_index = last.map(|row| row.0).unwrap_or(0);
    let mut ids = Vec::with_capacity(request.data.len());
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to add novel chapters"))?;
    for (idx, item) in request.data.into_iter().enumerate() {
        chapter_index += 1;
        let id = next_id(idx as i64);
        sqlx::query(
            r#"INSERT INTO toonflow.novels
               (id, chapter_index, reel, chapter, chapter_data, project_id, event_state, create_time)
               VALUES ($1,$2,$3,$4,$5,$6,0,$7)"#,
        )
        .bind(id)
        .bind(if item.index > 0 { item.index } else { chapter_index })
        .bind(item.reel)
        .bind(item.chapter)
        .bind(item.chapter_data)
        .bind(request.project_id)
        .bind(now_ms())
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to add novel chapter"))?;
        ids.push(id);
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to add novel chapters"))?;
    Ok(Json(ApiResponse::with_message(
        json!({ "ids": ids }),
        "新增原文成功",
    )))
}

pub async fn list_novel(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ListNovelRequest>,
) -> Result<Json<ApiResponse<PageData<NovelChapter>>>, AppError> {
    require(&user, "toon:project:read")?;
    let search = request.search.unwrap_or_default();
    let search_pattern = format!("%{search}%");
    let limit = request.limit.clamp(1, 200);
    let offset = (request.page.max(1) - 1) * limit;
    let rows = sqlx::query_as::<_, NovelChapter>(
        r#"SELECT id, chapter_index, reel, chapter, chapter_data, project_id, event_state,
                  event, error_reason, create_time
           FROM toonflow.novels
           WHERE project_id=$1 AND ($2 = '' OR chapter ILIKE $3)
           ORDER BY chapter_index ASC LIMIT $4 OFFSET $5"#,
    )
    .bind(request.project_id)
    .bind(&search)
    .bind(&search_pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list novel chapters"))?;
    let total: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM toonflow.novels WHERE project_id=$1 AND ($2 = '' OR chapter ILIKE $3)",
    )
    .bind(request.project_id)
    .bind(&search)
    .bind(&search_pattern)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to count novel chapters"))?;
    Ok(Json(ApiResponse::new(PageData {
        data: rows,
        total: total.0,
    })))
}

pub async fn all_novel(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ProjectIdRequest>,
) -> Result<Json<ApiResponse<Vec<NovelChapter>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, NovelChapter>(
        r#"SELECT id, chapter_index, reel, chapter, chapter_data, project_id, event_state,
                  event, error_reason, create_time
           FROM toonflow.novels WHERE project_id=$1 ORDER BY chapter_index ASC"#,
    )
    .bind(request.project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list novel chapters"))?;
    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNovelRequest {
    pub id: i64,
    #[serde(alias = "index")]
    pub chapter_index: i32,
    pub reel: String,
    pub chapter: String,
    pub chapter_data: String,
    pub event: Option<String>,
}

pub async fn update_novel(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<UpdateNovelRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let result = sqlx::query(
        r#"UPDATE toonflow.novels
           SET chapter_index=$2, reel=$3, chapter=$4, chapter_data=$5, event=$6
           WHERE id=$1"#,
    )
    .bind(request.id)
    .bind(request.chapter_index)
    .bind(request.reel)
    .bind(request.chapter)
    .bind(request.chapter_data)
    .bind(request.event)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to update novel chapter"))?;
    affected(result.rows_affected(), "novel")?;
    Ok(Json(ApiResponse::with_message((), "更新原文成功")))
}

pub async fn delete_novel(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<IdRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let result = sqlx::query("DELETE FROM toonflow.novels WHERE id=$1")
        .bind(request.id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete novel chapter"))?;
    affected(result.rows_affected(), "novel")?;
    Ok(Json(ApiResponse::with_message((), "删除原文成功")))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdRequest {
    pub project_id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptSummary {
    pub id: i64,
    pub name: String,
    pub content: String,
    pub project_id: i64,
    pub extract_state: Option<i32>,
    pub error_reason: Option<String>,
    pub create_time: i64,
    pub related_assets: Vec<Value>,
}

#[derive(Debug, FromRow)]
struct ScriptRow {
    id: i64,
    name: String,
    content: String,
    project_id: i64,
    extract_state: Option<i32>,
    error_reason: Option<String>,
    create_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListScriptRequest {
    pub project_id: i64,
    pub name: Option<String>,
}

pub async fn list_scripts(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ListScriptRequest>,
) -> Result<Json<ApiResponse<Vec<ScriptSummary>>>, AppError> {
    require(&user, "toon:episode:read")?;
    let name = request.name.unwrap_or_default();
    let pattern = format!("%{name}%");
    let scripts = sqlx::query_as::<_, ScriptRow>(
        r#"SELECT id, name, content, project_id, extract_state, error_reason, create_time
           FROM toonflow.scripts
           WHERE project_id=$1 AND ($2 = '' OR name ILIKE $3)
           ORDER BY create_time DESC"#,
    )
    .bind(request.project_id)
    .bind(&name)
    .bind(&pattern)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list scripts"))?;
    let ids: Vec<i64> = scripts.iter().map(|script| script.id).collect();
    let asset_rows = sqlx::query(
        r#"SELECT sa.script_id, a.id, a.name
           FROM toonflow.script_assets sa
           JOIN toonflow.assets a ON a.id = sa.asset_id
           WHERE sa.script_id = ANY($1)"#,
    )
    .bind(&ids)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list script assets"))?;
    let mut result = Vec::with_capacity(scripts.len());
    for script in scripts {
        let related_assets = asset_rows
            .iter()
            .filter(|row| row.get::<i64, _>("script_id") == script.id)
            .map(|row| json!({ "id": row.get::<i64, _>("id"), "name": row.get::<String, _>("name") }))
            .collect();
        result.push(ScriptSummary {
            id: script.id,
            name: script.name,
            content: script.content,
            project_id: script.project_id,
            extract_state: script.extract_state,
            error_reason: script.error_reason,
            create_time: script.create_time,
            related_assets,
        });
    }
    Ok(Json(ApiResponse::new(result)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveScriptRequest {
    pub id: Option<i64>,
    pub name: String,
    pub content: String,
    pub project_id: Option<i64>,
    #[serde(default)]
    pub assets: Vec<i64>,
}

pub async fn add_script(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveScriptRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:episode:create")?;
    let project_id = request
        .project_id
        .ok_or_else(|| AppError::bad_request("projectId is required"))?;
    let id = request.id.unwrap_or_else(|| next_id(0));
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to create script"))?;
    sqlx::query(
        "INSERT INTO toonflow.scripts (id, name, content, project_id, create_time) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(request.name)
    .bind(request.content)
    .bind(project_id)
    .bind(now_ms())
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to create script"))?;
    sync_script_assets(&mut tx, id, &request.assets).await?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to create script"))?;
    Ok(Json(ApiResponse::with_message(
        json!({ "id": id }),
        "添加剧本成功",
    )))
}

pub async fn update_script(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveScriptRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:episode:update")?;
    let id = request
        .id
        .ok_or_else(|| AppError::bad_request("script id is required"))?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to update script"))?;
    let result = sqlx::query("UPDATE toonflow.scripts SET name=$2, content=$3 WHERE id=$1")
        .bind(id)
        .bind(request.name)
        .bind(request.content)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to update script"))?;
    affected(result.rows_affected(), "script")?;
    sync_script_assets(&mut tx, id, &request.assets).await?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to update script"))?;
    Ok(Json(ApiResponse::with_message((), "编辑剧本成功")))
}

async fn sync_script_assets(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    script_id: i64,
    asset_ids: &[i64],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM toonflow.script_assets WHERE script_id=$1")
        .bind(script_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| AppError::internal("failed to update script assets"))?;
    for asset_id in asset_ids {
        sqlx::query(
            "INSERT INTO toonflow.script_assets (script_id, asset_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(script_id)
        .bind(asset_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| AppError::internal("failed to update script assets"))?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct DeleteScriptsRequest {
    pub ids: Vec<i64>,
}

pub async fn delete_scripts(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<DeleteScriptsRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:episode:delete")?;
    let result = sqlx::query("DELETE FROM toonflow.scripts WHERE id = ANY($1)")
        .bind(&request.ids)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete scripts"))?;
    affected(result.rows_affected(), "script")?;
    Ok(Json(ApiResponse::with_message((), "删除剧本成功")))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAddScriptRequest {
    pub project_id: i64,
    pub data: Vec<BatchScriptItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchScriptItem {
    pub script_name: String,
    pub script_data: String,
}

pub async fn batch_add_scripts(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<BatchAddScriptRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:episode:create")?;
    let mut ids = Vec::with_capacity(request.data.len());
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to create scripts"))?;
    for (idx, item) in request.data.into_iter().enumerate() {
        let id = next_id(idx as i64);
        sqlx::query(
            "INSERT INTO toonflow.scripts (id, name, content, project_id, create_time) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(id)
        .bind(item.script_name)
        .bind(item.script_data)
        .bind(request.project_id)
        .bind(now_ms())
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to create scripts"))?;
        ids.push(id);
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to create scripts"))?;
    Ok(Json(ApiResponse::with_message(
        json!({ "ids": ids }),
        "添加剧本成功",
    )))
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AssetRow {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub remark: Option<String>,
    #[serde(rename = "type")]
    pub type_: String,
    pub description: String,
    pub script_id: Option<i64>,
    pub image_id: Option<i64>,
    pub image_file_path: Option<String>,
    pub parent_asset_id: Option<i64>,
    pub project_id: i64,
    pub flow_id: Option<i64>,
    pub prompt_state: Option<String>,
    pub audio_bind_state: Option<i32>,
    pub prompt_error_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAssetRequest {
    pub id: Option<i64>,
    pub project_id: i64,
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    pub remark: Option<String>,
    #[serde(default)]
    pub r#type: String,
    #[serde(alias = "desc", alias = "describe", default)]
    pub description: String,
    pub script_id: Option<i64>,
    pub parent_asset_id: Option<i64>,
}

pub async fn list_assets(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ProjectIdRequest>,
) -> Result<Json<ApiResponse<Vec<AssetRow>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, AssetRow>(
        r#"SELECT a.id, a.name, a.prompt, a.remark, a.type as type_, a.description,
                  a.script_id, a.image_id, i.file_path as image_file_path,
                  a.parent_asset_id, a.project_id, a.flow_id, a.prompt_state,
                  a.audio_bind_state, a.prompt_error_reason
           FROM toonflow.assets a
           LEFT JOIN toonflow.images i ON i.id = a.image_id
           WHERE EXISTS (
             SELECT 1 FROM toonflow.project_assets pa
             WHERE pa.asset_id = a.id AND pa.project_id = $1
           )
             AND a.parent_asset_id IS NULL
           ORDER BY a.id DESC"#,
    )
    .bind(request.project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list assets"))?;
    Ok(Json(ApiResponse::new(rows)))
}

pub async fn save_asset(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveAssetRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let id = request.id.unwrap_or_else(|| next_id(0));
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to begin asset transaction"))?;
    sqlx::query(
        r#"INSERT INTO toonflow.assets
           (id, project_id, name, prompt, remark, type, description, script_id, parent_asset_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
           ON CONFLICT (id) DO UPDATE SET
             name=excluded.name, prompt=excluded.prompt, remark=excluded.remark, type=excluded.type,
             description=excluded.description, script_id=excluded.script_id,
             parent_asset_id=excluded.parent_asset_id"#,
    )
    .bind(id)
    .bind(request.project_id)
    .bind(request.name)
    .bind(request.prompt)
    .bind(request.remark)
    .bind(request.r#type)
    .bind(request.description)
    .bind(request.script_id)
    .bind(request.parent_asset_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to save asset"))?;
    sqlx::query(
        "INSERT INTO toonflow.project_assets(project_id,asset_id,linked_at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(request.project_id)
    .bind(id)
    .bind(now_ms())
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to link saved asset"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to commit asset"))?;
    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

#[derive(Debug, Deserialize)]
pub struct DeleteIdsRequest {
    pub ids: Option<Vec<i64>>,
    pub id: Option<Vec<i64>>,
}

pub async fn delete_assets(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<DeleteIdsRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let ids = request.ids.or(request.id).unwrap_or_default();
    if ids.is_empty() {
        return Err(AppError::bad_request("ids is required"));
    }
    let result = sqlx::query("DELETE FROM toonflow.assets WHERE id = ANY($1)")
        .bind(&ids)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete assets"))?;
    affected(result.rows_affected(), "asset")?;
    Ok(Json(ApiResponse::with_message((), "删除资产成功")))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRequest {
    pub project_id: i64,
    #[serde(alias = "episodesId")]
    pub episodes_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFlowRequest {
    pub project_id: i64,
    #[serde(alias = "episodesId")]
    pub episodes_id: i64,
    pub data: Value,
}

pub async fn get_flow_data(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<FlowRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:read")?;
    let flow: Option<(Value,)> = sqlx::query_as(
        "SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id=$2 AND key='productionAgent'",
    )
    .bind(request.project_id)
    .bind(request.episodes_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to get flow data"))?;
    let (script, assets) = crate::toonflow_asset_context::load_script_context(
        &state.pool,
        request.project_id,
        request.episodes_id,
    )
    .await
    .map_err(|_| AppError::internal("failed to build production asset context"))?;
    if let Some((mut data,)) = flow {
        data["script"] = json!(script);
        data["assets"] = assets;
        return Ok(Json(ApiResponse::new(data)));
    }
    Ok(Json(ApiResponse::new(json!({
        "script": script,
        "scriptPlan": "",
        "assets": assets,
        "storyboardTable": "",
        "storyboard": [],
        "workbench": { "videoList": [] }
    }))))
}

pub async fn save_flow_data(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveFlowRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:update")?;
    let time = now_ms();
    sqlx::query(
        r#"INSERT INTO toonflow.agent_work_data
           (project_id, episodes_id, key, data, create_time, update_time)
           VALUES ($1,$2,'productionAgent',$3,$4,$4)
           ON CONFLICT (project_id, episodes_id, key)
           DO UPDATE SET data=excluded.data, update_time=excluded.update_time"#,
    )
    .bind(request.project_id)
    .bind(request.episodes_id)
    .bind(request.data)
    .bind(time)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to save flow data"))?;
    Ok(Json(ApiResponse::new(())))
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardRow {
    pub id: i64,
    pub script_id: i64,
    pub prompt: String,
    pub file_path: Option<String>,
    pub duration: Option<String>,
    pub state: Option<String>,
    pub track_id: Option<i64>,
    pub reason: Option<String>,
    pub track: Option<String>,
    pub video_desc: Option<String>,
    pub should_generate_image: i32,
    pub project_id: i64,
    pub flow_id: Option<i64>,
    pub index: Option<i32>,
    pub create_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardListRequest {
    pub script_id: i64,
    pub project_id: i64,
}

pub async fn get_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<StoryboardListRequest>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows = sqlx::query_as::<_, StoryboardRow>(
        r#"SELECT id, script_id, prompt, file_path, duration, state, track_id, reason, track,
                  video_desc, should_generate_image, project_id, flow_id, index, create_time
           FROM toonflow.storyboards WHERE script_id=$1 AND project_id=$2 ORDER BY index ASC NULLS LAST, id ASC"#,
    )
    .bind(request.script_id)
    .bind(request.project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list storyboards"))?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let asset_ids: Vec<(i64,)> = sqlx::query_as(
            "SELECT asset_id FROM toonflow.assets_storyboards WHERE storyboard_id=$1 ORDER BY sort_order,asset_id",
        )
        .bind(row.id)
        .fetch_all(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to list storyboard assets"))?;
        result.push(json!({
            "id": row.id,
            "scriptId": row.script_id,
            "projectId": row.project_id,
            "prompt": row.prompt,
            "filePath": row.file_path,
            "src": row.file_path,
            "duration": row.duration.and_then(|value| value.parse::<i64>().ok()),
            "state": row.state,
            "trackId": row.track_id,
            "reason": row.reason,
            "track": row.track,
            "videoDesc": row.video_desc,
            "shouldGenerateImage": row.should_generate_image,
            "flowId": row.flow_id,
            "index": row.index,
            "createTime": row.create_time,
            "associateAssetsIds": asset_ids.into_iter().map(|id| id.0).collect::<Vec<_>>()
        }));
    }
    Ok(Json(ApiResponse::new(result)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStoryboardRequest {
    pub id: Option<i64>,
    pub prompt: String,
    pub duration: Option<i64>,
    #[serde(default)]
    pub state: String,
    pub video_desc: Option<String>,
    #[serde(default = "default_should_generate")]
    pub should_generate_image: i32,
    #[serde(alias = "src")]
    pub file_path: Option<String>,
    pub script_id: Option<i64>,
    pub project_id: Option<i64>,
    pub track: Option<String>,
    #[serde(default)]
    pub associate_assets_ids: Vec<i64>,
}

fn default_should_generate() -> i32 {
    1
}

pub async fn add_storyboard(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveStoryboardRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:create")?;
    let script_id = request
        .script_id
        .ok_or_else(|| AppError::bad_request("scriptId is required"))?;
    let project_id = request
        .project_id
        .ok_or_else(|| AppError::bad_request("projectId is required"))?;
    crate::toonflow_storyboard_asset_validation::reject_base_role_asset_ids(
        &state.pool,
        &request.associate_assets_ids,
    )
    .await?;
    let id = request.id.unwrap_or_else(|| next_id(0));
    let track_id = next_id(1);
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to create storyboard"))?;
    sqlx::query(
        "INSERT INTO toonflow.video_tracks (id, script_id, project_id, duration) VALUES ($1,$2,$3,$4)",
    )
    .bind(track_id)
    .bind(script_id)
    .bind(project_id)
    .bind(request.duration.map(|duration| duration as i32))
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to create storyboard track"))?;
    insert_storyboard(
        &mut tx,
        id,
        Some(track_id),
        &request,
        0,
        script_id,
        project_id,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to create storyboard"))?;
    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

async fn insert_storyboard(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    track_id: Option<i64>,
    request: &SaveStoryboardRequest,
    index: i32,
    script_id: i64,
    project_id: i64,
) -> Result<(), AppError> {
    sqlx::query(
        r#"INSERT INTO toonflow.storyboards
           (id, script_id, prompt, file_path, duration, state, track_id, track, video_desc,
            should_generate_image, project_id, index, create_time)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
    )
    .bind(id)
    .bind(script_id)
    .bind(&request.prompt)
    .bind(&request.file_path)
    .bind(request.duration.map(|value| value.to_string()))
    .bind(&request.state)
    .bind(track_id)
    .bind(&request.track)
    .bind(&request.video_desc)
    .bind(request.should_generate_image)
    .bind(project_id)
    .bind(index)
    .bind(now_ms())
    .execute(&mut **tx)
    .await
    .map_err(|_| AppError::internal("failed to create storyboard"))?;
    sync_storyboard_assets(tx, id, &request.associate_assets_ids).await?;
    Ok(())
}

async fn sync_storyboard_assets(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    storyboard_id: i64,
    asset_ids: &[i64],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM toonflow.assets_storyboards WHERE storyboard_id=$1")
        .bind(storyboard_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| AppError::internal("failed to update storyboard assets"))?;
    for (sort_order, asset_id) in asset_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO toonflow.assets_storyboards (storyboard_id, asset_id, sort_order) VALUES ($1,$2,$3) ON CONFLICT(storyboard_id,asset_id) DO UPDATE SET sort_order=excluded.sort_order",
        )
        .bind(storyboard_id)
        .bind(asset_id)
        .bind(sort_order as i32)
        .execute(&mut **tx)
        .await
        .map_err(|_| AppError::internal("failed to update storyboard assets"))?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct BatchStoryboardRequest {
    pub data: Vec<SaveStoryboardRequest>,
    #[serde(rename = "scriptId")]
    pub script_id: i64,
    #[serde(rename = "projectId")]
    pub project_id: i64,
}

pub async fn batch_add_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<BatchStoryboardRequest>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:create")?;
    if request.data.is_empty() {
        return Err(AppError::bad_request("data is required"));
    }
    let associated_asset_ids = request
        .data
        .iter()
        .flat_map(|item| item.associate_assets_ids.iter().copied())
        .collect::<Vec<_>>();
    crate::toonflow_storyboard_asset_validation::reject_base_role_asset_ids(
        &state.pool,
        &associated_asset_ids,
    )
    .await?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to create storyboards"))?;
    let mut storyboard_groups = std::collections::BTreeMap::<String, Vec<i64>>::new();
    for (idx, item) in request.data.iter().enumerate() {
        let id = item.id.unwrap_or_else(|| next_id(idx as i64));
        let track = item
            .track
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "main".to_string());
        insert_storyboard(
            &mut tx,
            id,
            None,
            item,
            idx as i32,
            request.script_id,
            request.project_id,
        )
        .await?;
        storyboard_groups.entry(track).or_default().push(id);
    }
    for (group_index, (track, storyboard_ids)) in storyboard_groups.iter().enumerate() {
        let existing_track_id: Option<i64> = sqlx::query_scalar(
            "SELECT track_id FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 AND track=$3 AND track_id IS NOT NULL LIMIT 1",
        )
        .bind(request.project_id)
        .bind(request.script_id)
        .bind(track)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to find storyboard track"))?;
        let track_id = existing_track_id.unwrap_or_else(|| next_id(10_000 + group_index as i64));
        let duration: i32 = sqlx::query_scalar(
            "SELECT coalesce(sum(CASE WHEN duration ~ '^[0-9]+$' THEN duration::integer ELSE 0 END),0)::integer FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 AND track=$3",
        )
        .bind(request.project_id)
        .bind(request.script_id)
        .bind(track)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to total storyboard track duration"))?;
        if existing_track_id.is_some() {
            sqlx::query("UPDATE toonflow.video_tracks SET duration=$2 WHERE id=$1")
                .bind(track_id)
                .bind(duration)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to update storyboard track"))?;
        } else {
            sqlx::query(
                "INSERT INTO toonflow.video_tracks(id,project_id,script_id,state,duration) VALUES($1,$2,$3,'未生成',$4)",
            )
            .bind(track_id)
            .bind(request.project_id)
            .bind(request.script_id)
            .bind(duration)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to create storyboard track"))?;
        }
        sqlx::query("UPDATE toonflow.storyboards SET track_id=$2 WHERE id=ANY($1)")
            .bind(storyboard_ids)
            .bind(track_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to bind storyboard track"))?;
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to create storyboards"))?;
    get_storyboards(
        user,
        State(state),
        Json(StoryboardListRequest {
            script_id: request.script_id,
            project_id: request.project_id,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditStoryboardInfoRequest {
    pub id: i64,
    pub prompt: String,
    pub video_desc: String,
    pub duration: Option<i64>,
    pub track: Option<String>,
    #[serde(default = "default_should_generate")]
    pub should_generate_image: i32,
    #[serde(default)]
    pub associate_assets_ids: Vec<i64>,
}

pub async fn edit_storyboard_info(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<EditStoryboardInfoRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:update")?;
    crate::toonflow_storyboard_asset_validation::reject_base_role_asset_ids(
        &state.pool,
        &request.associate_assets_ids,
    )
    .await?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to update storyboard"))?;
    let current: Option<(i64, i64, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT project_id,script_id,track_id,track FROM toonflow.storyboards WHERE id=$1",
    )
    .bind(request.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to load storyboard"))?;
    let Some((project_id, script_id, old_track_id, old_track)) = current else {
        return Err(AppError::not_found("storyboard not found"));
    };
    let track = request.track.as_deref().unwrap_or("main").trim();
    let track = if track.is_empty() { "main" } else { track };
    let target_track_id: Option<i64> = sqlx::query_scalar(
        "SELECT track_id FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 AND track=$3 AND id<>$4 AND track_id IS NOT NULL LIMIT 1",
    )
    .bind(project_id)
    .bind(script_id)
    .bind(track)
    .bind(request.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to find storyboard track"))?;
    let track_id = match target_track_id {
        Some(id) => id,
        None if old_track_id.is_some() && old_track.as_deref().unwrap_or("main") == track => {
            old_track_id.unwrap()
        }
        None => {
            let id = next_id(2);
            sqlx::query("INSERT INTO toonflow.video_tracks(id,project_id,script_id,state,duration) VALUES($1,$2,$3,'未生成',0)")
                .bind(id).bind(project_id).bind(script_id).execute(&mut *tx).await
                .map_err(|_| AppError::internal("failed to create storyboard track"))?;
            id
        }
    };
    let result = sqlx::query("UPDATE toonflow.storyboards SET prompt=$2,video_desc=$3,duration=$4,track=$5,track_id=$6,should_generate_image=$7 WHERE id=$1")
        .bind(request.id).bind(request.prompt).bind(request.video_desc)
        .bind(request.duration.map(|value| value.to_string())).bind(track).bind(track_id)
        .bind(request.should_generate_image).execute(&mut *tx).await
        .map_err(|_| AppError::internal("failed to update storyboard"))?;
    sqlx::query("DELETE FROM toonflow.assets_storyboards WHERE storyboard_id=$1")
        .bind(request.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to update storyboard assets"))?;
    for (sort_order, asset_id) in request.associate_assets_ids.iter().enumerate() {
        sqlx::query("INSERT INTO toonflow.assets_storyboards(storyboard_id,asset_id,sort_order) VALUES($1,$2,$3)")
            .bind(request.id).bind(asset_id).bind(sort_order as i32).execute(&mut *tx).await
            .map_err(|_| AppError::internal("failed to update storyboard assets"))?;
    }
    let mut affected_track_ids = vec![track_id];
    if let Some(old_track_id) = old_track_id.filter(|id| *id != track_id) {
        affected_track_ids.push(old_track_id);
    }
    for affected_track_id in affected_track_ids {
        let storyboard_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM toonflow.storyboards WHERE track_id=$1")
                .bind(affected_track_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to inspect storyboard track"))?;
        let duration: i32 = sqlx::query_scalar("SELECT coalesce(sum(CASE WHEN duration ~ '^[0-9]+$' THEN duration::integer ELSE 0 END),0)::integer FROM toonflow.storyboards WHERE track_id=$1")
            .bind(affected_track_id).fetch_one(&mut *tx).await
            .map_err(|_| AppError::internal("failed to total storyboard track duration"))?;
        if storyboard_count == 0 {
            sqlx::query("DELETE FROM toonflow.video_tracks WHERE id=$1")
                .bind(affected_track_id)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to delete empty storyboard track"))?;
        } else {
            sqlx::query("UPDATE toonflow.video_tracks SET duration=$2 WHERE id=$1")
                .bind(affected_track_id)
                .bind(duration)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::internal("failed to update storyboard track"))?;
        }
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to update storyboard"))?;
    affected(result.rows_affected(), "storyboard")?;
    Ok(Json(ApiResponse::with_message((), "更新分镜成功")))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderStoryboardsRequest {
    pub project_id: i64,
    pub script_id: i64,
    pub storyboard_ids: Vec<i64>,
}

pub async fn reorder_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ReorderStoryboardsRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:update")?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2",
    )
    .bind(request.project_id)
    .bind(request.script_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to validate storyboards"))?;
    let matched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 AND id=ANY($3)",
    )
    .bind(request.project_id)
    .bind(request.script_id)
    .bind(&request.storyboard_ids)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to validate storyboards"))?;
    if total != request.storyboard_ids.len() as i64 || matched != total {
        return Err(AppError::bad_request("分镜排序必须包含当前剧本的全部分镜"));
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    for (index, id) in request.storyboard_ids.into_iter().enumerate() {
        sqlx::query("UPDATE toonflow.storyboards SET index=$2 WHERE id=$1")
            .bind(id)
            .bind(index as i32)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to reorder storyboards"))?;
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    Ok(Json(ApiResponse::with_message((), "分镜排序已保存")))
}

pub async fn remove_storyboard(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<IdRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:delete")?;
    let row: Option<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT track_id,flow_id FROM toonflow.storyboards WHERE id=$1")
            .bind(request.id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to load storyboard"))?;
    let Some((track_id, flow_id)) = row else {
        return Err(AppError::not_found("storyboard not found"));
    };
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to begin transaction"))?;
    sqlx::query("DELETE FROM toonflow.assets_storyboards WHERE storyboard_id=$1")
        .bind(request.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to delete storyboard assets"))?;
    sqlx::query("DELETE FROM toonflow.storyboards WHERE id=$1")
        .bind(request.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to delete storyboard"))?;
    if let Some(flow_id) = flow_id {
        sqlx::query("DELETE FROM toonflow.image_flows WHERE id=$1")
            .bind(flow_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to delete storyboard image flow"))?;
    }
    if let Some(track_id) = track_id {
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
            sqlx::query(
                "UPDATE toonflow.video_tracks SET duration=(SELECT coalesce(sum(CASE WHEN duration ~ '^[0-9]+$' THEN duration::integer ELSE 0 END),0)::integer FROM toonflow.storyboards WHERE track_id=$1) WHERE id=$1",
            )
            .bind(track_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to update storyboard track"))?;
        }
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to commit storyboard deletion"))?;
    Ok(Json(ApiResponse::with_message((), "视频删除成功")))
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentDeployment {
    pub id: i64,
    pub key: String,
    pub description: String,
    pub name: String,
    pub temperature: i32,
    pub max_output_tokens: i32,
    pub disabled: bool,
    pub model_config_id: Option<i64>,
    pub model_type: String,
}

pub async fn list_agent_deployments(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<AgentDeployment>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, AgentDeployment>(
        r#"SELECT d.id,d.key,d.description,d.name,
                  d.temperature,d.max_output_tokens,d.disabled,d.model_config_id,d.model_type
           FROM toonflow.agent_deployments d ORDER BY d.id"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list agent deployments"))?;
    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentDeploymentRequest {
    pub id: i64,
    pub temperature: Option<i32>,
    pub max_output_tokens: Option<i32>,
    pub disabled: Option<bool>,
    pub model_config_id: Option<i64>,
}

pub async fn update_agent_deployment(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<UpdateAgentDeploymentRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let model_id = request
        .model_config_id
        .ok_or_else(|| AppError::bad_request("必须绑定统一 AI 模型"))?;
    let valid_model: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1
               FROM toonflow.agent_deployments d
               JOIN ai.model_configs m ON m.id=$2
               WHERE d.id=$1 AND m.status=0 AND m.type=d.model_type
           )"#,
    )
    .bind(request.id)
    .bind(model_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to validate agent model"))?;
    if !valid_model {
        return Err(AppError::bad_request("模型未启用或类型与当前用途不匹配"));
    }
    let result = sqlx::query(r#"UPDATE toonflow.agent_deployments SET temperature=coalesce($2,temperature),max_output_tokens=coalesce($3,max_output_tokens),disabled=coalesce($4,disabled),model_config_id=$5 WHERE id=$1"#)
    .bind(request.id)
    .bind(request.temperature)
    .bind(request.max_output_tokens)
    .bind(request.disabled)
    .bind(model_id)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to update agent deployment"))?;
    affected(result.rows_affected(), "agent deployment")?;
    Ok(Json(ApiResponse::new(())))
}

#[derive(Debug, Serialize, FromRow)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
}

pub async fn list_settings(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<SettingRow>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows =
        sqlx::query_as::<_, SettingRow>("SELECT key, value FROM toonflow.settings ORDER BY key")
            .fetch_all(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to list toonflow settings"))?;
    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize)]
pub struct SaveSettingRequest {
    pub key: String,
    pub value: String,
}

pub async fn save_setting(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveSettingRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    sqlx::query(
        r#"INSERT INTO toonflow.settings (key, value) VALUES ($1,$2)
           ON CONFLICT (key) DO UPDATE SET value=excluded.value"#,
    )
    .bind(request.key)
    .bind(request.value)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to save toonflow setting"))?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn health() -> Json<ApiResponse<Value>> {
    Json(ApiResponse::new(json!({
        "module": "toonflow",
        "capabilities": [
            "project",
            "novel",
            "script",
            "assets",
            "storyboard",
            "production-flow",
            "vendor-config",
            "agent-deploy"
        ]
    })))
}

pub async fn get_project_by_path(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<ToonflowProject>>, AppError> {
    require(&user, "toon:project:read")?;
    let row = sqlx::query_as::<_, ToonflowProject>(
        r#"SELECT id, project_type, image_model, image_quality, video_model, name, intro,
                  type as type_, art_style, director_manual, mode, video_ratio, create_time, update_time
           FROM toonflow.projects WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to get project"))?
    .ok_or_else(|| AppError::not_found("project not found"))?;
    Ok(Json(ApiResponse::new(row)))
}
