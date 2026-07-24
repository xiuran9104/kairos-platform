use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{ToonState, shared::require};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLibraryRequest {
    pub project_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAssetRequest {
    pub project_id: i64,
    pub asset_id: i64,
}

#[derive(Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAsset {
    id: i64,
    name: String,
    prompt: String,
    remark: Option<String>,
    #[serde(rename = "type")]
    type_: String,
    description: String,
    image_id: Option<i64>,
    image_file_path: Option<String>,
    parent_asset_id: Option<i64>,
    project_id: i64,
    source_project_name: String,
    linked_to_project: bool,
}

pub async fn list(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<AssetLibraryRequest>,
) -> Result<Json<ApiResponse<Vec<LibraryAsset>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, LibraryAsset>(
        r#"SELECT a.id, a.name, a.prompt, a.remark, a.type AS type_, a.description,
                  a.image_id, i.file_path AS image_file_path, a.parent_asset_id,
                  a.project_id, p.name AS source_project_name,
                  EXISTS(
                    SELECT 1 FROM toonflow.project_assets pa
                    WHERE pa.asset_id = a.id AND pa.project_id = $1
                  ) AS linked_to_project
           FROM toonflow.assets a
           JOIN toonflow.projects p ON p.id = a.project_id
           LEFT JOIN toonflow.images i ON i.id = a.image_id
           WHERE a.parent_asset_id IS NULL
           ORDER BY a.id DESC"#,
    )
    .bind(request.project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to list shared assets"))?;
    Ok(Json(ApiResponse::new(rows)))
}

pub async fn link(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ProjectAssetRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let asset_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM toonflow.assets WHERE id=$1 AND parent_asset_id IS NULL)",
    )
    .bind(request.asset_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to validate asset"))?;
    if !asset_exists {
        return Err(AppError::not_found("asset not found"));
    }
    sqlx::query(
        r#"INSERT INTO toonflow.project_assets(project_id, asset_id, linked_at)
           SELECT $1, a.id, $3 FROM toonflow.assets a
           WHERE a.id = $2
           ON CONFLICT DO NOTHING"#,
    )
    .bind(request.project_id)
    .bind(request.asset_id)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to link asset"))?;
    Ok(Json(ApiResponse::with_message((), "资产已引用")))
}

pub async fn unlink(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ProjectAssetRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:project:update")?;
    let owner_project_id: Option<i64> =
        sqlx::query_scalar("SELECT project_id FROM toonflow.assets WHERE id=$1")
            .bind(request.asset_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to validate asset ownership"))?;
    if owner_project_id == Some(request.project_id) {
        return Err(AppError::bad_request("来源项目不能取消引用自己的资产"));
    }
    sqlx::query("DELETE FROM toonflow.project_assets WHERE project_id=$1 AND asset_id=$2")
        .bind(request.project_id)
        .bind(request.asset_id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to unlink asset"))?;
    Ok(Json(ApiResponse::with_message((), "已取消资产引用")))
}
