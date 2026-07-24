use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_toon_api::{CreateSceneRequest, SceneSummary, UpdateSceneRequest};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    ToonState,
    shared::{affected, parse_id, require, validate_status},
};

#[derive(FromRow)]
struct SceneRow {
    id: Uuid,
    episode_id: Uuid,
    title: String,
    scene_no: i32,
    content: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn summary(row: SceneRow) -> SceneSummary {
    SceneSummary {
        id: row.id.to_string(),
        episode_id: row.episode_id.to_string(),
        title: row.title,
        scene_no: row.scene_no,
        content: row.content,
        status: row.status,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

pub async fn list(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(episode_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<SceneSummary>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows = sqlx::query_as::<_, SceneRow>(
        "SELECT id, episode_id, title, scene_no, content, status, created_at, updated_at
         FROM toon.scenes WHERE episode_id = $1 ORDER BY scene_no",
    )
    .bind(parse_id(&episode_id)?)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list scenes"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter().map(summary).collect(),
    )))
}

pub async fn create(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(episode_id): Path<String>,
    Json(request): Json<CreateSceneRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    require(&user, "toon:scene:create")?;
    if request.title.trim().is_empty() || request.scene_no <= 0 {
        return Err(AppError::bad_request("invalid scene"));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO toon.scenes (id, episode_id, title, scene_no, content)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(parse_id(&episode_id)?)
    .bind(request.title.trim())
    .bind(request.scene_no)
    .bind(request.content)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::bad_request("episode does not exist or scene number already exists"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

pub async fn update(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSceneRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:update")?;
    validate_status(&request.status)?;
    let result = sqlx::query(
        "UPDATE toon.scenes
         SET title = $2, scene_no = $3, content = $4, status = $5, updated_at = now()
         WHERE id = $1",
    )
    .bind(parse_id(&id)?)
    .bind(request.title.trim())
    .bind(request.scene_no)
    .bind(request.content)
    .bind(request.status)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::bad_request("failed to update scene"))?;
    affected(result.rows_affected(), "scene")?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn delete(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:delete")?;
    let result = sqlx::query("DELETE FROM toon.scenes WHERE id = $1")
        .bind(parse_id(&id)?)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete scene"))?;
    affected(result.rows_affected(), "scene")?;
    Ok(Json(ApiResponse::new(())))
}
