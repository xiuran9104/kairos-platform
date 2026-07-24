use crate::{ToonState, ai_client, shared::require};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn next_id(offset: i64) -> i64 {
    now_ms() * 1000 + offset
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    project_id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindRequest {
    assets_id: i64,
    audio_ids: Option<Vec<i64>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchBindRequest {
    project_id: i64,
    assets_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct PollRequest {
    ids: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DubbingRequest {
    project_id: i64,
    assets_id: i64,
    text: String,
    #[serde(default = "default_voice")]
    voice: String,
    model: Option<String>,
}

fn default_voice() -> String {
    "alloy".to_string()
}

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AudioAssetRow {
    id: i64,
    name: String,
    description: String,
    #[serde(rename = "type")]
    type_: String,
    project_id: i64,
    audio_bind_state: Option<i32>,
    audio_url: Option<String>,
    releped_audio: Value,
}

pub async fn all_assets(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ProjectRequest>,
) -> Result<Json<ApiResponse<Vec<AudioAssetRow>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, AudioAssetRow>(
        r#"SELECT a.id,a.name,a.description,a.type as type_,a.project_id,a.audio_bind_state,
                  i.file_path as audio_url,
                  COALESCE((SELECT jsonb_agg(jsonb_build_object('id',aa.id,'name',aa.name,'url',ai.file_path))
                    FROM toonflow.asset_audio_bindings b JOIN toonflow.assets aa ON aa.id=b.asset_audio_id
                    LEFT JOIN toonflow.images ai ON ai.id=aa.image_id WHERE b.asset_role_id=a.id),'[]'::jsonb) as releped_audio
           FROM toonflow.assets a LEFT JOIN toonflow.images i ON i.id=a.image_id
           WHERE a.project_id=$1 ORDER BY a.id DESC"#,
    ).bind(request.project_id).fetch_all(&state.pool).await
        .map_err(|_| AppError::internal("failed to list audio assets"))?;
    Ok(Json(ApiResponse::new(rows)))
}

pub async fn update_binding(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<BindRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to bind audio"))?;
    sqlx::query("DELETE FROM toonflow.asset_audio_bindings WHERE asset_role_id=$1")
        .bind(request.assets_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to bind audio"))?;
    if let Some(audio_id) = request.audio_ids.unwrap_or_default().into_iter().next() {
        sqlx::query("INSERT INTO toonflow.asset_audio_bindings(asset_role_id,asset_audio_id,create_time) VALUES($1,$2,$3)")
            .bind(request.assets_id).bind(audio_id).bind(now_ms()).execute(&mut *tx).await
            .map_err(|_| AppError::bad_request("音色资产不存在或不可绑定"))?;
    }
    sqlx::query("UPDATE toonflow.assets SET audio_bind_state=1 WHERE id=$1")
        .bind(request.assets_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to bind audio"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to bind audio"))?;
    Ok(Json(ApiResponse::new(json!(true))))
}

pub async fn batch_bind(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<BatchBindRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let audio_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM toonflow.assets WHERE project_id=$1 AND type='audio' ORDER BY id",
    )
    .bind(request.project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to load audio assets"))?;
    let audio_id = audio_ids
        .first()
        .copied()
        .ok_or_else(|| AppError::bad_request("请先创建或生成音色资产"))?;
    for asset_id in request.assets_ids {
        sqlx::query("INSERT INTO toonflow.asset_audio_bindings(asset_role_id,asset_audio_id,create_time) VALUES($1,$2,$3) ON CONFLICT(asset_role_id,asset_audio_id) DO NOTHING")
            .bind(asset_id).bind(audio_id).bind(now_ms()).execute(&state.pool).await.map_err(|_| AppError::internal("failed to bind audio"))?;
        sqlx::query("UPDATE toonflow.assets SET audio_bind_state=1 WHERE id=$1 AND project_id=$2")
            .bind(asset_id)
            .bind(request.project_id)
            .execute(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to update audio state"))?;
    }
    Ok(Json(ApiResponse::new(json!({"audioId":audio_id}))))
}

pub async fn poll(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<PollRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows: Vec<(i64, Option<i32>)> =
        sqlx::query_as("SELECT id,audio_bind_state FROM toonflow.assets WHERE id=ANY($1)")
            .bind(&request.ids)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to poll audio"))?;
    Ok(Json(ApiResponse::new(json!(
        rows.into_iter()
            .map(|(id, state)| json!({"id":id,"audioBindState":state}))
            .collect::<Vec<_>>()
    ))))
}

pub async fn generate_dubbing(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<DubbingRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    if request.text.trim().is_empty() {
        return Err(AppError::bad_request("配音文本不能为空"));
    }
    let configured = match request.model {
        Some(model) if !model.is_empty() => model,
        _ => {
            let deployment: Option<(Option<i64>,)> = sqlx::query_as(
                "SELECT model_config_id FROM toonflow.agent_deployments WHERE key='ttsDubbing' AND disabled=false",
            ).fetch_optional(&state.pool).await.map_err(|_| AppError::internal("failed to load tts model"))?;
            deployment
                .ok_or_else(|| AppError::bad_request("请先配置 ttsDubbing Agent"))?
                .0
                .ok_or_else(|| AppError::bad_request("ttsDubbing Agent 必须绑定统一语音模型"))?
                .to_string()
        }
    };
    sqlx::query("UPDATE toonflow.assets SET audio_bind_state=2 WHERE id=$1 AND project_id=$2")
        .bind(request.assets_id)
        .bind(request.project_id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update audio state"))?;
    let url = match ai_client::speech(&state.pool, &configured, &request.text, &request.voice).await
    {
        Ok(url) => url,
        Err(error) => {
            sqlx::query(
                "UPDATE toonflow.assets SET audio_bind_state=-1,prompt_error_reason=$2 WHERE id=$1",
            )
            .bind(request.assets_id)
            .bind(&error)
            .execute(&state.pool)
            .await
            .ok();
            return Err(AppError::bad_request(error));
        }
    };
    let audio_id = next_id(1);
    let image_id = next_id(2);
    let name: String = sqlx::query_scalar("SELECT name FROM toonflow.assets WHERE id=$1")
        .bind(request.assets_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to load asset"))?
        .unwrap_or_else(|| "角色".into());
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to save dubbing"))?;
    sqlx::query("INSERT INTO toonflow.assets(id,name,prompt,type,description,parent_asset_id,project_id) VALUES($1,$2,$3,'audio',$4,$5,$6)")
        .bind(audio_id).bind(format!("{name}-配音")).bind(&request.text).bind(format!("voice:{}",request.voice)).bind(request.assets_id).bind(request.project_id)
        .execute(&mut *tx).await.map_err(|_| AppError::internal("failed to save dubbing"))?;
    sqlx::query("INSERT INTO toonflow.images(id,file_path,type,assets_id,model,state) VALUES($1,$2,'audio',$3,$4,'生成完成')")
        .bind(image_id).bind(&url).bind(audio_id).bind(&configured).execute(&mut *tx).await.map_err(|_| AppError::internal("failed to save dubbing"))?;
    sqlx::query("UPDATE toonflow.assets SET image_id=$2 WHERE id=$1")
        .bind(audio_id)
        .bind(image_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to select dubbing"))?;
    sqlx::query("INSERT INTO toonflow.asset_audio_bindings(asset_role_id,asset_audio_id,create_time) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(request.assets_id).bind(audio_id).bind(now_ms()).execute(&mut *tx).await.map_err(|_| AppError::internal("failed to bind dubbing"))?;
    sqlx::query(
        "UPDATE toonflow.assets SET audio_bind_state=1,prompt_error_reason=NULL WHERE id=$1",
    )
    .bind(request.assets_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to finish dubbing"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to save dubbing"))?;
    Ok(Json(ApiResponse::new(json!({"id":audio_id,"url":url}))))
}
