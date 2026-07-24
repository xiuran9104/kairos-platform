use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use kairos_platform_media_api::{AssetSummary, CreateAssetRequest, UpdateAssetRequest};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    MediaState,
    shared::{affected, current_user_id, parse_id, require},
};

#[derive(FromRow)]
struct AssetRow {
    id: Uuid,
    object_key: String,
    filename: Option<String>,
    content_type: String,
    size_bytes: i64,
    status: String,
    checksum: Option<String>,
    metadata: Value,
    owner_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

fn summary(row: AssetRow) -> AssetSummary {
    AssetSummary {
        id: row.id.to_string(),
        object_key: row.object_key,
        filename: row.filename,
        content_type: row.content_type,
        size_bytes: row.size_bytes,
        status: row.status,
        checksum: row.checksum,
        metadata: row.metadata,
        owner_user_id: row.owner_user_id.map(|id| id.to_string()),
        created_at: row.created_at.to_rfc3339(),
    }
}

pub async fn list(
    user: CurrentUser,
    State(state): State<MediaState>,
) -> Result<Json<ApiResponse<Vec<AssetSummary>>>, AppError> {
    require(&user, "media:asset:read")?;
    let rows = sqlx::query_as::<_, AssetRow>(
        "SELECT id, object_key, filename, content_type, size_bytes, status,
                checksum, metadata, owner_user_id, created_at
         FROM media.assets ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list assets"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter().map(summary).collect(),
    )))
}

pub async fn get(
    user: CurrentUser,
    State(state): State<MediaState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<AssetSummary>>, AppError> {
    require(&user, "media:asset:read")?;
    let row = sqlx::query_as::<_, AssetRow>(
        "SELECT id, object_key, filename, content_type, size_bytes, status,
                checksum, metadata, owner_user_id, created_at
         FROM media.assets WHERE id = $1",
    )
    .bind(parse_id(&id)?)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to get asset"))?
    .ok_or_else(|| AppError::not_found("asset not found"))?;
    Ok(Json(ApiResponse::new(summary(row))))
}

pub async fn create(
    user: CurrentUser,
    State(state): State<MediaState>,
    Json(request): Json<CreateAssetRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    require(&user, "media:asset:create")?;
    if request.object_key.trim().is_empty()
        || request.content_type.trim().is_empty()
        || request.size_bytes < 0
    {
        return Err(AppError::bad_request("invalid asset"));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO media.assets
         (id, object_key, filename, content_type, size_bytes, checksum, metadata, owner_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(request.object_key.trim())
    .bind(request.filename)
    .bind(request.content_type.trim())
    .bind(request.size_bytes)
    .bind(request.checksum)
    .bind(request.metadata)
    .bind(current_user_id(&user)?)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to create asset"))?;
    Ok(Json(ApiResponse::new(id.to_string())))
}

pub async fn update(
    user: CurrentUser,
    State(state): State<MediaState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAssetRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "media:asset:update")?;
    let result = sqlx::query(
        "UPDATE media.assets
         SET filename = $2, status = $3, checksum = $4, metadata = $5
         WHERE id = $1",
    )
    .bind(parse_id(&id)?)
    .bind(request.filename)
    .bind(request.status)
    .bind(request.checksum)
    .bind(request.metadata)
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to update asset"))?;
    affected(result.rows_affected(), "asset")?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn delete(
    user: CurrentUser,
    State(state): State<MediaState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "media:asset:delete")?;
    let result = sqlx::query("DELETE FROM media.assets WHERE id = $1")
        .bind(parse_id(&id)?)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete asset"))?;
    affected(result.rows_affected(), "asset")?;
    Ok(Json(ApiResponse::new(())))
}
