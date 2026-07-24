use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_toon_api::{CreateEpisodeRequest, EpisodeSummary, UpdateEpisodeRequest};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    ToonState,
    shared::{affected, parse_id, require, validate_status},
};

#[derive(FromRow)]
struct EpisodeRow {
    id: Uuid,
    project_id: Uuid,
    title: String,
    episode_no: i32,
    summary: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn summary(row: EpisodeRow) -> EpisodeSummary {
    EpisodeSummary {
        id: row.id.to_string(),
        project_id: row.project_id.to_string(),
        title: row.title,
        episode_no: row.episode_no,
        summary: row.summary,
        status: row.status,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

pub async fn list(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(project_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<EpisodeSummary>>>, AppError> {
    require(&user, "toon:episode:read")?;
    let rows = sqlx::query_as::<_, EpisodeRow>(
        "SELECT id, project_id, title, episode_no, summary, status, created_at, updated_at
         FROM toon.episodes WHERE project_id = $1 ORDER BY episode_no",
    )
    .bind(parse_id(&project_id)?)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list episodes"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter().map(summary).collect(),
    )))
}

pub async fn create(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateEpisodeRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    require(&user, "toon:episode:create")?;
    if request.title.trim().is_empty() || request.episode_no <= 0 {
        return Err(AppError::bad_request("invalid episode"));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO toon.episodes (id, project_id, title, episode_no, summary)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(parse_id(&project_id)?)
    .bind(request.title.trim())
    .bind(request.episode_no)
    .bind(request.summary)
    .execute(&state.pool)
    .await
    .map_err(|_| {
        AppError::bad_request("project does not exist or episode number already exists")
    })?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

pub async fn update(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateEpisodeRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:episode:update")?;
    validate_status(&request.status)?;
    let result = sqlx::query(
        "UPDATE toon.episodes
         SET title = $2, episode_no = $3, summary = $4, status = $5, updated_at = now()
         WHERE id = $1",
    )
    .bind(parse_id(&id)?)
    .bind(request.title.trim())
    .bind(request.episode_no)
    .bind(request.summary)
    .bind(request.status)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::bad_request("failed to update episode"))?;
    affected(result.rows_affected(), "episode")?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn delete(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:episode:delete")?;
    let result = sqlx::query("DELETE FROM toon.episodes WHERE id = $1")
        .bind(parse_id(&id)?)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete episode"))?;
    affected(result.rows_affected(), "episode")?;
    Ok(Json(ApiResponse::new(())))
}
