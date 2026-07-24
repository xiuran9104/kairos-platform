use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

use crate::{ToonState, shared::require};

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CreativeManual {
    id: i64,
    kind: String,
    name: String,
    path: String,
    images: Value,
    data: Value,
    create_time: i64,
    update_time: i64,
}

async fn list_kind(state: &ToonState, kind: &str) -> Result<Vec<CreativeManual>, AppError> {
    sqlx::query_as::<_, CreativeManual>("SELECT id,kind,name,path,images,data,create_time,update_time FROM toonflow.creative_manuals WHERE kind=$1 ORDER BY name")
        .bind(kind).fetch_all(&state.pool).await.map_err(|_| AppError::internal("failed to list creative manuals"))
}

pub async fn list_all(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<CreativeManual>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows=sqlx::query_as::<_,CreativeManual>("SELECT id,kind,name,path,images,data,create_time,update_time FROM toonflow.creative_manuals ORDER BY kind,name")
        .fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list creative manuals"))?;
    Ok(Json(ApiResponse::new(rows)))
}

pub async fn list_visual(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let data=list_kind(&state,"visual").await?.into_iter().map(|item|serde_json::json!({"id":item.id,"name":item.name,"stylePath":item.path,"image":item.images,"data":item.data})).collect();
    Ok(Json(ApiResponse::new(data)))
}

pub async fn list_director(
    user: CurrentUser,
    State(state): State<ToonState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let data=list_kind(&state,"director").await?.into_iter().map(|item|serde_json::json!({"id":item.id,"name":item.name,"directorManual":item.path,"image":item.images,"data":item.data})).collect();
    Ok(Json(ApiResponse::new(data)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveManualRequest {
    name: String,
    images: Vec<String>,
    data: Value,
    style_path: Option<String>,
    director_manual: Option<String>,
}

fn validate_name(name: &str) -> Result<(), AppError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().all(|c| c.is_ascii_digit())
        || name.contains('/')
        || name.contains('\\')
    {
        return Err(AppError::bad_request("名称不能包含路径分隔符或为纯数字"));
    }
    Ok(())
}

async fn save(
    user: &CurrentUser,
    state: &ToonState,
    request: SaveManualRequest,
    kind: &str,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(user, "toon:project:update")?;
    validate_name(&request.name)?;
    let path = request
        .style_path
        .or(request.director_manual)
        .unwrap_or_else(|| request.name.clone());
    validate_name(&path)?;
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO toonflow.creative_manuals (kind,name,path,images,data,create_time,update_time) VALUES ($1,$2,$3,$4,$5,$6,$6) ON CONFLICT (kind,path) DO UPDATE SET name=excluded.name,images=excluded.images,data=excluded.data,update_time=excluded.update_time")
        .bind(kind).bind(request.name).bind(path).bind(serde_json::json!(request.images)).bind(request.data).bind(now)
        .execute(&state.pool).await.map_err(|_|AppError::internal("failed to save creative manual"))?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn save_visual(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveManualRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    save(&user, &state, request, "visual").await
}
pub async fn save_director(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SaveManualRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    save(&user, &state, request, "director").await
}

#[derive(Debug, Deserialize)]
pub struct DeleteManualRequest {
    name: String,
}
async fn delete(
    user: &CurrentUser,
    state: &ToonState,
    request: DeleteManualRequest,
    kind: &str,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(user, "toon:project:update")?;
    validate_name(&request.name)?;
    sqlx::query("DELETE FROM toonflow.creative_manuals WHERE kind=$1 AND path=$2")
        .bind(kind)
        .bind(request.name)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete creative manual"))?;
    Ok(Json(ApiResponse::new(
        serde_json::json!({"message":"删除成功"}),
    )))
}
pub async fn delete_visual(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<DeleteManualRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    delete(&user, &state, request, "visual").await
}
pub async fn delete_director(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<DeleteManualRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    delete(&user, &state, request, "director").await
}
