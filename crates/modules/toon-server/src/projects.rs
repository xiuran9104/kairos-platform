use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_toon_api::{
    CreateProjectRequest, ProjectSummary, PublishProjectRequest, UpdateProjectRequest,
};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    ToonState,
    shared::{affected, current_user_id, parse_id, require, validate_status},
};

#[derive(FromRow)]
struct ProjectRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    status: String,
    owner_user_id: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn summary(row: ProjectRow) -> ProjectSummary {
    ProjectSummary {
        id: row.id.to_string(),
        name: row.name,
        description: row.description,
        status: row.status,
        owner_user_id: row.owner_user_id.to_string(),
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

pub async fn list(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<ProjectSummary>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, ProjectRow>(
        "SELECT id, name, description, status, owner_user_id, created_at, updated_at
         FROM toon.projects ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list projects"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter().map(summary).collect(),
    )))
}

pub async fn get(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ProjectSummary>>, AppError> {
    require(&user, "toon:project:read")?;
    let row = sqlx::query_as::<_, ProjectRow>(
        "SELECT id, name, description, status, owner_user_id, created_at, updated_at
         FROM toon.projects WHERE id = $1",
    )
    .bind(parse_id(&id)?)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to get project"))?
    .ok_or_else(|| AppError::not_found("project not found"))?;
    Ok(Json(ApiResponse::new(summary(row))))
}

pub async fn create(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    require(&user, "toon:project:create")?;
    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("project name is required"));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO toon.projects (id, name, description, owner_user_id)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(request.name.trim())
    .bind(request.description)
    .bind(current_user_id(&user)?)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to create project"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

pub async fn update(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    validate_status(&request.status)?;
    let result = sqlx::query(
        "UPDATE toon.projects
         SET name = $2, description = $3, status = $4, updated_at = now()
         WHERE id = $1",
    )
    .bind(parse_id(&id)?)
    .bind(request.name.trim())
    .bind(request.description)
    .bind(request.status)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to update project"))?;
    affected(result.rows_affected(), "project")?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn delete(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:delete")?;
    let result = sqlx::query("DELETE FROM toon.projects WHERE id = $1")
        .bind(parse_id(&id)?)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete project"))?;
    affected(result.rows_affected(), "project")?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn publish(
    user: CurrentUser,
    State(state): State<ToonState>,
    Path(id): Path<String>,
    Json(request): Json<PublishProjectRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    require(&user, "toon:project:publish")?;
    if request.channel.trim().is_empty() {
        return Err(AppError::bad_request("publish channel is required"));
    }
    let publication_id = Uuid::new_v4();
    let project_id = parse_id(&id)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to publish project"))?;
    let updated = sqlx::query(
        "UPDATE toon.projects SET status = 'published', updated_at = now() WHERE id = $1",
    )
    .bind(project_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to publish project"))?
    .rows_affected();
    affected(updated, "project")?;
    sqlx::query("INSERT INTO toon.publications (id, project_id, channel, created_by) VALUES ($1, $2, $3, $4)")
        .bind(publication_id)
        .bind(project_id)
        .bind(request.channel.trim())
        .bind(current_user_id(&user)?)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to publish project"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to publish project"))?;
    Ok(Json(ApiResponse::new(publication_id.to_string())))
}
