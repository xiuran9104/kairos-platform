use crate::{ToonState, ai_client, shared::require};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAssetsRequest {
    assets_ids: Vec<i64>,
}

pub async fn audio_bind_assets(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<AudioAssetsRequest>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows=sqlx::query_as::<_,(i64,String,String,Option<String>,i64)>("SELECT audio.id,audio.prompt,audio.type,i.file_path,b.asset_role_id FROM toonflow.asset_audio_bindings b JOIN toonflow.assets audio ON audio.id=b.asset_audio_id LEFT JOIN toonflow.images i ON i.id=audio.image_id WHERE b.asset_role_id=ANY($1) ORDER BY audio.id").bind(req.assets_ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list bound audio assets"))?;
    Ok(Json(ApiResponse::new(rows.into_iter().map(|row|json!({"fileType":"audio","sources":"assets","src":row.3,"id":row.0,"prompt":row.1,"type":row.2,"assetsRoleId":row.4})).collect())))
}

#[derive(Deserialize)]
pub struct FileItem {
    id: i64,
    sources: String,
}

#[derive(Deserialize)]
pub struct FileUrlRequest {
    items: Vec<FileItem>,
}

pub async fn file_urls(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<FileUrlRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:read")?;
    let storyboard_ids = req
        .items
        .iter()
        .filter(|item| item.sources == "storyboard")
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let asset_ids = req
        .items
        .iter()
        .filter(|item| item.sources == "assets")
        .map(|item| item.id)
        .collect::<Vec<_>>();
    let mut result = serde_json::Map::new();
    if !storyboard_ids.is_empty() {
        let rows = sqlx::query_as::<_, (i64, Option<String>)>(
            "SELECT id,file_path FROM toonflow.storyboards WHERE id=ANY($1)",
        )
        .bind(storyboard_ids)
        .fetch_all(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to load storyboard files"))?;
        for (id, path) in rows {
            result.insert(format!("{id}:storyboard"), json!(path.unwrap_or_default()));
        }
    }
    if !asset_ids.is_empty() {
        let rows=sqlx::query_as::<_,(i64,Option<String>)>("SELECT a.id,i.file_path FROM toonflow.assets a LEFT JOIN toonflow.images i ON i.id=a.image_id WHERE a.id=ANY($1)").bind(asset_ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to load asset files"))?;
        for (id, path) in rows {
            result.insert(format!("{id}:assets"), json!(path.unwrap_or_default()));
        }
    }
    Ok(Json(ApiResponse::new(json!({"data":result}))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRequest {
    project_id: i64,
    script_id: i64,
    duration: Option<i32>,
}
pub async fn add_track(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<TrackRequest>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    require(&user, "toon:scene:update")?;
    let id = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO toonflow.video_tracks(id,project_id,script_id,duration,state,sort_order) VALUES($1,$2,$3,$4,'未生成',coalesce((SELECT max(sort_order)+1 FROM toonflow.video_tracks WHERE project_id=$2 AND script_id=$3),0))").bind(id).bind(req.project_id).bind(req.script_id).bind(req.duration).execute(&state.pool).await.map_err(|_|AppError::internal("failed to add video track"))?;
    Ok(Json(ApiResponse::new(id)))
}
#[derive(Deserialize)]
pub struct Id {
    id: i64,
}
pub async fn delete_track(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Id>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:delete")?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    sqlx::query("UPDATE toonflow.storyboards SET track_id=NULL WHERE track_id=$1")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to unbind track"))?;
    sqlx::query("DELETE FROM toonflow.video_tracks WHERE id=$1")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to delete track"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    Ok(Json(ApiResponse::new(json!({"message":"视频段删除成功"}))))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workbench {
    project_id: i64,
    script_id: i64,
}
pub async fn video_list(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Workbench>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows=sqlx::query_as::<_,(i64,Option<String>,String,Option<String>,Option<i64>)>("SELECT id,file_path,coalesce(state,''),error_reason,video_track_id FROM toonflow.videos WHERE project_id=$1 AND script_id=$2 ORDER BY id DESC").bind(req.project_id).bind(req.script_id).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list videos"))?;
    Ok(Json(ApiResponse::new(rows.into_iter().map(|r|json!({"id":r.0,"filePath":r.1,"src":r.1,"state":r.2,"errorReason":r.3,"videoTrackId":r.4})).collect())))
}
#[derive(Deserialize)]
pub struct Prompt {
    id: i64,
    prompt: Option<String>,
}
pub async fn update_prompt(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Prompt>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    require(&user, "toon:scene:update")?;
    sqlx::query("UPDATE toonflow.video_tracks SET prompt=$2 WHERE id=$1")
        .bind(req.id)
        .bind(req.prompt)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update prompt"))?;
    Ok(Json(ApiResponse::new("更新成功")))
}
#[derive(Deserialize)]
pub struct Duration {
    id: i64,
    duration: Option<i32>,
}
pub async fn update_duration(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Duration>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    require(&user, "toon:scene:update")?;
    sqlx::query("UPDATE toonflow.video_tracks SET duration=$2 WHERE id=$1")
        .bind(req.id)
        .bind(req.duration)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update duration"))?;
    Ok(Json(ApiResponse::new("更新成功")))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Select {
    track_id: i64,
    video_id: i64,
}
pub async fn select_video(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Select>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:update")?;
    sqlx::query("UPDATE toonflow.video_tracks SET video_id=$2 WHERE id=$1")
        .bind(req.track_id)
        .bind(req.video_id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to select video"))?;
    Ok(Json(ApiResponse::new(json!({"message":"视频选择成功"}))))
}
pub async fn delete_video(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Id>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:delete")?;
    sqlx::query("UPDATE toonflow.video_tracks SET video_id=NULL WHERE video_id=$1")
        .bind(req.id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to unbind video"))?;
    sqlx::query("DELETE FROM toonflow.videos WHERE id=$1")
        .bind(req.id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete video"))?;
    Ok(Json(ApiResponse::new(json!({"message":"视频删除成功"}))))
}

pub async fn generate_data(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Workbench>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:read")?;
    Ok(Json(ApiResponse::new(
        load_generate_data(&state.pool, req.project_id, req.script_id).await?,
    )))
}

pub(crate) async fn load_generate_data(
    pool: &sqlx::PgPool,
    project_id: i64,
    script_id: i64,
) -> Result<Value, AppError> {
    let boards=sqlx::query_as::<_,(i64,Option<i64>,Option<String>,String,Option<String>,i32)>("SELECT id,track_id,file_path,prompt,video_desc,coalesce(index,0) FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 ORDER BY index,id").bind(project_id).bind(script_id).fetch_all(pool).await.map_err(|_|AppError::internal("failed to list storyboards"))?;
    let tracks=sqlx::query_as::<_,(i64,Option<String>,Option<String>,Option<i32>,Option<i64>,i32)>("SELECT id,prompt,state,duration,video_id,sort_order FROM toonflow.video_tracks WHERE project_id=$1 AND script_id=$2 ORDER BY sort_order,id").bind(project_id).bind(script_id).fetch_all(pool).await.map_err(|_|AppError::internal("failed to list tracks"))?;
    let videos=sqlx::query_as::<_,(i64,Option<String>,String,Option<String>,Option<i64>)>("SELECT id,file_path,coalesce(state,''),error_reason,video_track_id FROM toonflow.videos WHERE project_id=$1 AND script_id=$2").bind(project_id).bind(script_id).fetch_all(pool).await.map_err(|_|AppError::internal("failed to list videos"))?;
    let list=tracks.into_iter().map(|t|json!({"id":t.0,"prompt":t.1,"state":t.2,"duration":t.3,"selectVideoId":t.4,"sortOrder":t.5,"medias":boards.iter().filter(|b|b.1==Some(t.0)).map(|b|json!({"id":b.0,"src":b.2,"prompt":b.4,"fileType":"image","sources":"storyboard","index":b.5})).collect::<Vec<_>>(),"videoList":videos.iter().filter(|v|v.4==Some(t.0)).map(|v|json!({"id":v.0,"src":v.1,"state":v.2,"errorReason":v.3})).collect::<Vec<_>>() })).collect::<Vec<_>>();
    Ok(
        json!({"storyboardList":boards.into_iter().map(|b|json!({"id":b.0,"trackId":b.1,"src":b.2,"prompt":b.3,"videoDesc":b.4,"index":b.5})).collect::<Vec<_>>(),"trackList":list}),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generate {
    project_id: i64,
    script_id: i64,
    prompt: String,
    #[serde(deserialize_with = "deserialize_model")]
    model: String,
    mode: Value,
    resolution: String,
    duration: i32,
    audio: Option<bool>,
    track_id: i64,
    upload_data: Value,
}

/// Combines caller-provided frames with canonical asset images while preserving their first-use order.
fn merge_references(upload_data: Value, asset_references: Vec<String>) -> Value {
    let mut references = upload_data
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str()
                .map(str::to_string)
                .or_else(|| item.get("src").and_then(Value::as_str).map(str::to_string))
        })
        .filter(|reference| !reference.is_empty())
        .map(Value::String)
        .collect::<Vec<_>>();
    for reference in asset_references {
        if !references
            .iter()
            .any(|item| item.as_str() == Some(&reference))
        {
            references.push(json!(reference));
        }
    }
    // Current video providers accept at most four ordered visual inputs. Storyboard frames from
    // `upload_data` stay first; canonical character/scene references fill the remaining slots.
    references.truncate(4);
    json!(references)
}

fn references_for_mode(upload_data: Value, asset_references: Vec<String>, mode: &Value) -> Value {
    let mode = mode.as_str().unwrap_or("text");
    if mode == "text" {
        return merge_references(upload_data, asset_references);
    }
    let frames = merge_references(upload_data, Vec::new())
        .as_array()
        .cloned()
        .unwrap_or_default();
    if matches!(mode, "startEndRequired" | "endFrameOptional") && frames.len() > 1 {
        return json!([
            frames.first().cloned().unwrap(),
            frames.last().cloned().unwrap()
        ]);
    }
    json!(frames.into_iter().take(1).collect::<Vec<_>>())
}

pub async fn generate_video(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Generate>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    require(&user, "toon:scene:update")?;
    let id = chrono::Utc::now().timestamp_millis();
    let asset_references = crate::toonflow_asset_context::load_track_asset_references(
        &state.pool,
        req.project_id,
        req.script_id,
        req.track_id,
    )
    .await
    .map_err(|_| AppError::internal("failed to load video asset references"))?;
    let references = references_for_mode(req.upload_data, asset_references, &req.mode);
    sqlx::query("INSERT INTO toonflow.videos(id,state,script_id,project_id,video_track_id,time)VALUES($1,'生成中',$2,$3,$4,$1)").bind(id).bind(req.script_id).bind(req.project_id).bind(req.track_id).execute(&state.pool).await.map_err(|_|AppError::internal("failed to create video"))?;
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let ratio: Option<(String,)> =
            sqlx::query_as("SELECT video_ratio FROM toonflow.projects WHERE id=$1")
                .bind(req.project_id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();
        let payload = json!({"prompt":req.prompt,"mode":req.mode,"resolution":req.resolution,"duration":req.duration,"audio":req.audio.unwrap_or(false),"aspect_ratio":ratio.map(|r|r.0).unwrap_or_else(||"16:9".into()),"references":references});
        match ai_client::video(&pool, &req.model, payload).await {
            Ok(url) => {
                let _ = sqlx::query(
                    "UPDATE toonflow.videos SET file_path=$2,state='生成成功' WHERE id=$1 AND state='生成中'",
                )
                .bind(id)
                .bind(url)
                .execute(&pool)
                .await;
            }
            Err(reason) => {
                let _ = sqlx::query(
                    "UPDATE toonflow.videos SET state='生成失败',error_reason=$2 WHERE id=$1",
                )
                .bind(id)
                .bind(reason)
                .execute(&pool)
                .await;
            }
        }
    });
    Ok(Json(ApiResponse::new(id)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRequest {
    project_id: i64,
    script_id: i64,
    track_ids: Vec<i64>,
}

pub async fn reorder_tracks(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<ReorderRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:update")?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM toonflow.video_tracks WHERE project_id=$1 AND script_id=$2 AND id=ANY($3)")
        .bind(req.project_id).bind(req.script_id).bind(&req.track_ids).fetch_one(&state.pool).await.map_err(|_|AppError::internal("failed to validate tracks"))?;
    if count != req.track_ids.len() as i64 {
        return Err(AppError::bad_request("轨道列表包含无效 ID"));
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    for (index, id) in req.track_ids.into_iter().enumerate() {
        sqlx::query("UPDATE toonflow.video_tracks SET sort_order=$2 WHERE id=$1")
            .bind(id)
            .bind(index as i32)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::internal("failed to reorder tracks"))?;
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    Ok(Json(ApiResponse::new(())))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindStoryboardsRequest {
    track_id: i64,
    storyboard_ids: Vec<i64>,
}

pub async fn bind_storyboards(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<BindStoryboardsRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:scene:update")?;
    let track: Option<(i64, Option<i64>)> =
        sqlx::query_as("SELECT project_id,script_id FROM toonflow.video_tracks WHERE id=$1")
            .bind(req.track_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to load track"))?;
    let (project_id, script_id) = track.ok_or_else(|| AppError::not_found("track not found"))?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    let mut affected_track_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT track_id FROM toonflow.storyboards WHERE id=ANY($1) AND track_id IS NOT NULL",
    )
    .bind(&req.storyboard_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to load previous tracks"))?;
    affected_track_ids.push(req.track_id);
    affected_track_ids.sort_unstable();
    affected_track_ids.dedup();
    let result=sqlx::query("UPDATE toonflow.storyboards SET track_id=$1 WHERE id=ANY($2) AND project_id=$3 AND script_id=$4").bind(req.track_id).bind(&req.storyboard_ids).bind(project_id).bind(script_id).execute(&mut *tx).await.map_err(|_|AppError::internal("failed to bind storyboards"))?;
    if result.rows_affected() != req.storyboard_ids.len() as u64 {
        return Err(AppError::bad_request("部分分镜不存在或不属于当前轨道项目"));
    }
    sqlx::query(
        "UPDATE toonflow.video_tracks vt SET duration=coalesce((SELECT sum(CASE WHEN s.duration ~ '^[0-9]+$' THEN s.duration::integer ELSE 0 END)::integer FROM toonflow.storyboards s WHERE s.track_id=vt.id),0) WHERE vt.id=ANY($1)",
    )
    .bind(&affected_track_ids)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::internal("failed to update track durations"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed transaction"))?;
    Ok(Json(ApiResponse::new(())))
}

pub async fn cancel_video(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Id>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:scene:update")?;
    let result=sqlx::query("UPDATE toonflow.videos SET state='已取消',error_reason='用户取消生成' WHERE id=$1 AND state='生成中'").bind(req.id).execute(&state.pool).await.map_err(|_|AppError::internal("failed to cancel video"))?;
    Ok(Json(ApiResponse::new(
        json!({"canceled":result.rows_affected()>0}),
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryVideoRequest {
    id: i64,
    #[serde(deserialize_with = "deserialize_model")]
    model: String,
    mode: Value,
    resolution: String,
    audio: Option<bool>,
    upload_data: Value,
}

pub async fn retry_video(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<RetryVideoRequest>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    require(&user, "toon:scene:update")?;
    let source:Option<(i64,i64,Option<i64>,Option<String>,Option<i32>)>=sqlx::query_as("SELECT project_id,script_id,video_track_id,(SELECT prompt FROM toonflow.video_tracks WHERE id=video_track_id),(SELECT duration FROM toonflow.video_tracks WHERE id=video_track_id) FROM toonflow.videos WHERE id=$1 AND state IN('生成失败','已取消')").bind(req.id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to load retry video"))?;
    let (project_id, script_id, track_id, prompt, duration) =
        source.ok_or_else(|| AppError::bad_request("只有失败或已取消的视频可以重试"))?;
    let track_id = track_id.ok_or_else(|| AppError::bad_request("视频未关联轨道"))?;
    generate_video(
        user,
        State(state),
        Json(Generate {
            project_id,
            script_id,
            prompt: prompt.unwrap_or_default(),
            model: req.model,
            mode: req.mode,
            resolution: req.resolution,
            duration: duration.unwrap_or(5),
            audio: req.audio,
            track_id,
            upload_data: req.upload_data,
        }),
    )
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    project_id: i64,
    script_id: i64,
    video_ids: Vec<i64>,
}
pub async fn check_states(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Check>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows=sqlx::query_as::<_,(i64,String,Option<String>,Option<String>)>("SELECT id,state,error_reason,file_path FROM toonflow.videos WHERE project_id=$1 AND script_id=$2 AND id=ANY($3) AND state IN('生成成功','生成失败')").bind(req.project_id).bind(req.script_id).bind(req.video_ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to check videos"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .map(|r| json!({"id":r.0,"state":r.1,"errorReason":r.2,"filePath":r.3,"src":r.3}))
            .collect(),
    )))
}

pub(crate) async fn create_prompt(
    pool: &sqlx::PgPool,
    track_id: i64,
    project_id: i64,
    model: &str,
    mode: &str,
) -> Result<String, String> {
    sqlx::query("UPDATE toonflow.video_tracks SET state='生成中',reason=NULL WHERE id=$1")
        .bind(track_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    let style: Option<(String,)> =
        sqlx::query_as("SELECT art_style FROM toonflow.projects WHERE id=$1")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let resolved_model = if let Ok(model_id) = model.parse::<i64>() {
        sqlx::query_scalar::<_, String>("SELECT model FROM ai.model_configs WHERE id=$1")
            .bind(model_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| model.to_string())
    } else {
        model.to_string()
    };
    let prompt_name = video_prompt_name(&resolved_model, mode);
    let base:Option<(String,Option<String>)>=sqlx::query_as("SELECT data,use_data FROM toonflow.prompts WHERE source_key IS NOT NULL OR type='videoPromptGeneration' ORDER BY CASE WHEN source_key=$1 THEN 0 WHEN source_key='universal_multi_parameter' THEN 1 ELSE 2 END,id LIMIT 1").bind(prompt_name).fetch_optional(pool).await.map_err(|e|e.to_string())?;
    let system = base
        .map(|r| r.0)
        .unwrap_or_else(|| "根据分镜生成专业视频提示词，只输出提示词正文。".into());
    let system = format!(
        "{system}\n\n## 输出语言（最高优先级）\n最终视频提示词必须全部使用简体中文。标题、画面、动作、运镜、情绪、音效和时间段描述都必须是中文；台词保持原文。忽略上文任何英文输出要求，禁止输出 [Visual]、[Motion]、[Camera]、No dialogue 等英文标题或标签。"
    );
    let manual: Option<(Value,)> = sqlx::query_as(
        "SELECT data FROM toonflow.creative_manuals WHERE kind='visual' AND path=$1",
    )
    .bind(style.map(|r| r.0).unwrap_or_default())
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let visual = manual
        .and_then(|r| {
            r.0.as_array().and_then(|a| {
                a.iter()
                    .find(|v| {
                        v.get("value").and_then(Value::as_str) == Some("art_storyboard_video")
                    })
                    .and_then(|v| v.get("data"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        })
        .unwrap_or_default();
    let boards=sqlx::query_as::<_,(String,Option<String>,Option<String>)>("SELECT prompt,video_desc,duration FROM toonflow.storyboards WHERE track_id=$1 ORDER BY index,id").bind(track_id).fetch_all(pool).await.map_err(|e|e.to_string())?;
    let content = format!(
        "模型：{resolved_model}\n模式：{mode}\n视觉规范：{visual}\n{}",
        boards
            .into_iter()
            .map(|b| format!(
                "画面={},运动={},时长={}",
                b.0,
                b.1.unwrap_or_default(),
                b.2.unwrap_or_default()
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    match ai_client::text(pool, "universalAi", &system, &content).await {
        Ok(text) => {
            sqlx::query(
                "UPDATE toonflow.video_tracks SET prompt=$2,state='已完成',reason=NULL WHERE id=$1",
            )
            .bind(track_id)
            .bind(&text)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(text)
        }
        Err(reason) => {
            let _ = sqlx::query(
                "UPDATE toonflow.video_tracks SET state='生成失败',reason=$2 WHERE id=$1",
            )
            .bind(track_id)
            .bind(&reason)
            .execute(pool)
            .await;
            Err(reason)
        }
    }
}

fn video_prompt_name(model: &str, mode: &str) -> &'static str {
    let model = model.to_ascii_lowercase();
    if model.contains("wan") && model.contains("2.6") {
        "wan_2_6_single_image_first_frame"
    } else if model.contains("seedance")
        && (model.contains("2.0") || model.contains("2-0") || model.contains("2_0"))
    {
        "seedance_2_multi_parameter"
    } else if matches!(
        mode,
        "startEndRequired" | "endFrameOptional" | "startFrameOptional"
    ) {
        "universal_first_last_frame"
    } else {
        "universal_multi_parameter"
    }
}

fn model_parameter(value: &Value) -> Result<String, AppError> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .ok_or_else(|| AppError::bad_request("视频模型参数必须是模型 ID 或名称"))
}

#[cfg(test)]
mod prompt_tests {
    use super::{
        Generate, merge_references, model_parameter, references_for_mode, video_prompt_name,
    };
    use serde_json::json;

    #[test]
    fn selects_the_original_toonflow_video_prompt_variants() {
        assert_eq!(
            video_prompt_name("doubao-seedance-2-0-260128", "text"),
            "seedance_2_multi_parameter"
        );
        assert_eq!(
            video_prompt_name("wan2.6", "text"),
            "wan_2_6_single_image_first_frame"
        );
        assert_eq!(
            video_prompt_name("doubao-seedance-1-5-pro", "startEndRequired"),
            "universal_first_last_frame"
        );
        assert_eq!(
            video_prompt_name("doubao-seedance-1-5-pro", "text"),
            "universal_multi_parameter"
        );
    }

    #[test]
    fn accepts_numeric_and_string_model_parameters() {
        assert_eq!(model_parameter(&json!(123)).unwrap(), "123");
        assert_eq!(model_parameter(&json!("seedance")).unwrap(), "seedance");
        assert!(model_parameter(&json!({})).is_err());
    }

    #[test]
    fn video_generation_accepts_numeric_model_id() {
        let request: Generate = serde_json::from_value(json!({
            "projectId": 1,
            "scriptId": 2,
            "prompt": "镜头提示词",
            "model": 1784249635985_i64,
            "mode": "startEndRequired",
            "resolution": "1080p",
            "duration": 5,
            "trackId": 3,
            "uploadData": []
        }))
        .unwrap();
        assert_eq!(request.model, "1784249635985");
    }

    #[test]
    fn normalizes_storyboard_media_and_appends_unique_asset_references() {
        let references = merge_references(
            json!([
                {"id": 1, "src": "https://example.com/storyboard.png"},
                "https://example.com/direct.png"
            ]),
            vec![
                "https://example.com/role.png".into(),
                "https://example.com/direct.png".into(),
            ],
        );
        assert_eq!(
            references,
            json!([
                "https://example.com/storyboard.png",
                "https://example.com/direct.png",
                "https://example.com/role.png"
            ])
        );
    }

    #[test]
    fn limits_video_references_to_provider_maximum() {
        let references = merge_references(
            json!(["frame-1", "frame-2"]),
            vec!["role-1".into(), "scene-1".into(), "tool-1".into()],
        );
        assert_eq!(
            references,
            json!(["frame-1", "frame-2", "role-1", "scene-1"])
        );
    }

    #[test]
    fn separates_frame_modes_from_multi_reference_mode() {
        let frames = json!(["first", "middle", "last"]);
        let assets = vec!["role".into(), "scene".into()];
        assert_eq!(
            references_for_mode(frames.clone(), assets.clone(), &json!("startEndRequired")),
            json!(["first", "last"])
        );
        assert_eq!(
            references_for_mode(frames.clone(), assets.clone(), &json!("singleImage")),
            json!(["first"])
        );
        assert_eq!(
            references_for_mode(frames, assets, &json!("text")),
            json!(["first", "middle", "last", "role"])
        );
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptGenerate {
    track_id: i64,
    project_id: i64,
    info: Value,
    model: Value,
    mode: String,
}
pub async fn generate_prompt(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<PromptGenerate>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    require(&user, "toon:scene:update")?;
    let _ = req.info;
    let model = model_parameter(&req.model)?;
    let text = create_prompt(&state.pool, req.track_id, req.project_id, &model, &req.mode)
        .await
        .map_err(AppError::bad_request)?;
    Ok(Json(ApiResponse::new(text)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckPrompt {
    project_id: i64,
    script_id: i64,
    track_ids: Vec<i64>,
}
pub async fn check_prompts(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<CheckPrompt>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:read")?;
    let rows=sqlx::query_as::<_,(i64,String,Option<String>,Option<String>)>("SELECT id,coalesce(state,''),reason,prompt FROM toonflow.video_tracks WHERE project_id=$1 AND script_id=$2 AND id=ANY($3) AND state IN('已完成','生成失败')").bind(req.project_id).bind(req.script_id).bind(req.track_ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to check prompts"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .map(|r| json!({"id":r.0,"state":r.1,"reason":r.2,"prompt":r.3}))
            .collect(),
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPrompt {
    track_id: i64,
    info: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPrompts {
    project_id: i64,
    track_data: Vec<TrackPrompt>,
    mode: String,
    model: Value,
    concurrent_count: Option<usize>,
}
pub async fn batch_prompts(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<BatchPrompts>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    require(&user, "toon:scene:update")?;
    let model = model_parameter(&req.model)?;
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(
            req.concurrent_count.unwrap_or(5).clamp(1, 20),
        ));
        for track in req.track_data {
            let _ = track.info;
            let permit = sem.clone().acquire_owned().await;
            let pool = pool.clone();
            let model = model.clone();
            let mode = req.mode.clone();
            let project_id = req.project_id;
            tokio::spawn(async move {
                if permit.is_ok() {
                    let _ = create_prompt(&pool, track.track_id, project_id, &model, &mode).await;
                }
            });
        }
    });
    Ok(Json(ApiResponse::new("开始生成提示词")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoBatchItem {
    upload_data: Value,
    track_id: i64,
    prompt: String,
    duration: i32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchVideos {
    project_id: i64,
    script_id: i64,
    track_data: Vec<VideoBatchItem>,
    #[serde(deserialize_with = "deserialize_model")]
    model: String,
    mode: Value,
    resolution: String,
    audio: Option<bool>,
}

fn deserialize_model<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .ok_or_else(|| D::Error::custom("视频模型参数必须是模型 ID 或名称"))
}
pub async fn batch_videos(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<BatchVideos>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:scene:update")?;
    let mut jobs = Vec::new();
    for (index, mut track) in req.track_data.into_iter().enumerate() {
        let id = chrono::Utc::now().timestamp_millis() + index as i64;
        let asset_references = crate::toonflow_asset_context::load_track_asset_references(
            &state.pool,
            req.project_id,
            req.script_id,
            track.track_id,
        )
        .await
        .map_err(|_| AppError::internal("failed to load video asset references"))?;
        track.upload_data = references_for_mode(track.upload_data, asset_references, &req.mode);
        sqlx::query("INSERT INTO toonflow.videos(id,state,script_id,project_id,video_track_id,time)VALUES($1,'生成中',$2,$3,$4,$1)").bind(id).bind(req.script_id).bind(req.project_id).bind(track.track_id).execute(&state.pool).await.map_err(|_|AppError::internal("failed to create video"))?;
        jobs.push((id, track));
    }
    let response = jobs
        .iter()
        .map(|j| json!({"videoId":j.0,"trackId":j.1.track_id}))
        .collect();
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let ratio: Option<(String,)> =
            sqlx::query_as("SELECT video_ratio FROM toonflow.projects WHERE id=$1")
                .bind(req.project_id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();
        let ratio = ratio.map(|r| r.0).unwrap_or_else(|| "16:9".into());
        for (id, track) in jobs {
            let payload = json!({"prompt":track.prompt,"mode":req.mode,"resolution":req.resolution,"duration":track.duration,"audio":req.audio.unwrap_or(false),"aspect_ratio":ratio,"references":track.upload_data});
            match ai_client::video(&pool, &req.model, payload).await {
                Ok(url) => {
                    let _ = sqlx::query(
                        "UPDATE toonflow.videos SET file_path=$2,state='生成成功' WHERE id=$1",
                    )
                    .bind(id)
                    .bind(url)
                    .execute(&pool)
                    .await;
                }
                Err(reason) => {
                    let _ = sqlx::query(
                        "UPDATE toonflow.videos SET state='生成失败',error_reason=$2 WHERE id=$1",
                    )
                    .bind(id)
                    .bind(reason)
                    .execute(&pool)
                    .await;
                }
            }
        }
    });
    Ok(Json(ApiResponse::new(response)))
}
