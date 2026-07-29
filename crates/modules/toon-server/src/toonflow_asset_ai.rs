use crate::{
    ToonState, ai_client,
    shared::require,
    toonflow_asset_prompt, toonflow_prompt_store,
    toonflow_storage::{image_data_url, persist_remote_image},
};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolishItem {
    assets_id: i64,
    #[serde(rename = "type")]
    type_: String,
    name: String,
    describe: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolishRequest {
    assets_id: i64,
    project_id: i64,
    #[serde(rename = "type")]
    type_: String,
    name: String,
    describe: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    items: Vec<PolishItem>,
    project_id: i64,
    concurrent_count: Option<usize>,
    other_text_prompt: String,
}
fn manual_key(kind: &str, derivative: bool) -> Option<(&'static str, &'static str)> {
    match kind {
        "role" => Some((
            "角色",
            if derivative {
                "art_character_derivative"
            } else {
                "art_character"
            },
        )),
        "scene" => Some((
            "场景",
            if derivative {
                "art_scene_derivative"
            } else {
                "art_scene"
            },
        )),
        "tool" => Some((
            "道具",
            if derivative {
                "art_prop_derivative"
            } else {
                "art_prop"
            },
        )),
        "costume" => Some((
            "服装",
            if derivative {
                "art_character_derivative"
            } else {
                "art_character"
            },
        )),
        _ => None,
    }
}

async fn run(
    pool: &sqlx::PgPool,
    project_id: i64,
    item: PolishItem,
    extra: &str,
) -> Result<String, String> {
    sqlx::query(
        "UPDATE toonflow.assets SET prompt_state='生成中',prompt_error_reason=NULL WHERE id=$1",
    )
    .bind(item.assets_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    let context:Option<(String,Option<i64>)>=sqlx::query_as("SELECT p.art_style,a.parent_asset_id FROM toonflow.assets a JOIN toonflow.projects p ON p.id=a.project_id WHERE a.id=$1 AND a.project_id=$2").bind(item.assets_id).bind(project_id).fetch_optional(pool).await.map_err(|e|e.to_string())?;
    let (manual_path, parent) = context.ok_or_else(|| "资产不存在".to_string())?;
    let (label, key) =
        manual_key(&item.type_, parent.is_some()).ok_or_else(|| "不支持的类型".to_string())?;
    let data: Option<(Value,)> = sqlx::query_as(
        "SELECT data FROM toonflow.creative_manuals WHERE kind='visual' AND path=$1",
    )
    .bind(manual_path)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let system = data
        .and_then(|r| {
            r.0.as_array().and_then(|items| {
                items
                    .iter()
                    .find(|v| v.get("value").and_then(Value::as_str) == Some(key))
                    .and_then(|v| v.get("data"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "视觉手册未定义".to_string())?;
    let prompt = ai_client::text(
        pool,
        "universalAi",
        &toonflow_asset_prompt::polish_system_prompt(&system, extra, &item.type_, parent.is_some()),
        &toonflow_asset_prompt::polish_user_prompt(label, &item.name, &item.describe),
    )
    .await?;
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("AI 润色未生成可用的资产提示词".to_string());
    }
    sqlx::query("UPDATE toonflow.assets SET prompt=$2,prompt_state='已完成',prompt_error_reason=NULL WHERE id=$1").bind(item.assets_id).bind(&prompt).execute(pool).await.map_err(|e|e.to_string())?;
    Ok(prompt)
}
async fn mark_failed(pool: &sqlx::PgPool, id: i64, reason: &str) {
    let _ = sqlx::query(
        "UPDATE toonflow.assets SET prompt_state='失败',prompt_error_reason=$2 WHERE id=$1",
    )
    .bind(id)
    .bind(reason)
    .execute(pool)
    .await;
}

pub(crate) async fn polish_extracted_assets(
    pool: &sqlx::PgPool,
    project_id: i64,
    asset_ids: &[i64],
    concurrent_count: usize,
) -> Result<(), String> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        r#"SELECT id,type,name,coalesce(description,'')
           FROM toonflow.assets
           WHERE project_id=$1 AND id=ANY($2) AND type=ANY($3)
             AND coalesce(prompt,'')=''"#,
    )
    .bind(project_id)
    .bind(asset_ids)
    .bind(vec!["role", "scene", "tool", "costume"])
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrent_count.clamp(1, 10)));
    let mut jobs = tokio::task::JoinSet::new();
    for (assets_id, type_, name, describe) in rows {
        let pool = pool.clone();
        let permit = semaphore.clone().acquire_owned().await;
        jobs.spawn(async move {
            let Ok(_permit) = permit else { return };
            let item = PolishItem {
                assets_id,
                type_,
                name,
                describe,
            };
            if let Err(reason) = run(&pool, project_id, item, "").await {
                mark_failed(&pool, assets_id, &reason).await;
            }
        });
    }
    while jobs.join_next().await.is_some() {}
    Ok(())
}

pub async fn polish(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<PolishRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let item = PolishItem {
        assets_id: req.assets_id,
        type_: req.type_,
        name: req.name,
        describe: req.describe,
    };
    match run(&state.pool, req.project_id, item.clone(), "").await {
        Ok(prompt) => Ok(Json(ApiResponse::new(
            json!({"prompt":prompt,"assetsId":item.assets_id}),
        ))),
        Err(reason) => {
            mark_failed(&state.pool, item.assets_id, &reason).await;
            Err(AppError::bad_request(reason))
        }
    }
}
pub async fn batch_polish(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let total = req.items.len();
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(
            req.concurrent_count.unwrap_or(1).clamp(1, 20),
        ));
        let mut jobs = Vec::new();
        for item in req.items {
            let pool = pool.clone();
            let extra = req.other_text_prompt.clone();
            let permit = sem.clone().acquire_owned().await;
            let project_id = req.project_id;
            jobs.push(tokio::spawn(async move {
                if permit.is_ok() {
                    if let Err(reason) = run(&pool, project_id, item.clone(), &extra).await {
                        mark_failed(&pool, item.assets_id, &reason).await;
                    }
                }
            }));
        }
        for job in jobs {
            let _ = job.await;
        }
    });
    Ok(Json(ApiResponse::new(json!({"total":total}))))
}
#[derive(Deserialize)]
pub struct Ids {
    ids: Vec<i64>,
}
pub async fn poll_prompts(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Ids>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows=sqlx::query_as::<_,(i64,String,String,Option<String>)>("SELECT id,prompt,coalesce(prompt_state,''),prompt_error_reason FROM toonflow.assets WHERE id=ANY($1) AND coalesce(prompt_state,'')<>'生成中'").bind(req.ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to poll asset prompts"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .map(|r| json!({"id":r.0,"prompt":r.1,"promptState":r.2,"promptErrorReason":r.3}))
            .collect(),
    )))
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageItem {
    pub(crate) id: i64,
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) name: String,
    pub(crate) prompt: String,
    pub(crate) base64: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum ModelId {
    Number(i64),
    Text(String),
}

impl ModelId {
    fn as_configured(&self) -> String {
        match self {
            Self::Number(id) => id.to_string(),
            Self::Text(id) => id.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImageRequest {
    project_id: i64,
    model: ModelId,
    resolution: String,
    id: i64,
    #[serde(rename = "type")]
    type_: String,
    name: String,
    prompt: String,
    base64: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImageRequest {
    project_id: i64,
    model: ModelId,
    resolution: String,
    concurrent_count: Option<usize>,
    items: Vec<ImageItem>,
}
async fn new_image(
    pool: &sqlx::PgPool,
    item: &ImageItem,
    model: &str,
    resolution: &str,
    offset: i64,
    seed: i64,
) -> Result<i64, AppError> {
    let id = chrono::Utc::now().timestamp_millis() + offset;
    sqlx::query("INSERT INTO toonflow.images(id,type,assets_id,model,resolution,state,seed) VALUES($1,$2,$3,$4,$5,'生成中',$6)")
        .bind(id).bind(&item.type_).bind(item.id).bind(model).bind(resolution).bind(seed)
        .execute(pool).await.map_err(|_|AppError::internal("failed to create image"))?;
    Ok(id)
}
async fn make_image(
    pool: &sqlx::PgPool,
    project_id: i64,
    model: &str,
    resolution: &str,
    item: ImageItem,
    image_id: i64,
    seed: i64,
) -> Result<String, String> {
    let style: Option<(String,)> =
        sqlx::query_as("SELECT art_style FROM toonflow.projects WHERE id=$1")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let style = style.ok_or_else(|| "项目为空".to_string())?.0;
    let derivative: bool =
        sqlx::query_scalar("SELECT parent_asset_id IS NOT NULL FROM toonflow.assets WHERE id=$1")
            .bind(item.id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(false);
    let visual_description = item.prompt.clone();
    let prompt_key = match (item.type_.as_str(), derivative) {
        ("role", false) => "asset_image_role_base",
        ("role", true) => "asset_image_role_derivative",
        ("scene", _) => "asset_image_scene",
        ("tool", _) => "asset_image_tool",
        ("costume", _) => "asset_image_costume",
        _ => "",
    };
    let managed_instruction = if prompt_key.is_empty() {
        None
    } else {
        Some(toonflow_prompt_store::load(pool, prompt_key, "").await)
    };
    let mut prompt = toonflow_asset_prompt::image_prompt_with_instruction(
        &style,
        &item.type_,
        &visual_description,
        derivative,
        item.base64.is_some(),
        managed_instruction.as_deref(),
    );
    // AI-04: inject art style prefix for consistency
    let style_prefix = toonflow_prompt_store::load_style_prefix(pool, project_id).await;
    prompt = toonflow_prompt_store::inject_style_prefix(&prompt, &style_prefix);
    let references = item.base64.clone().into_iter().collect();
    match ai_client::image_with_references(pool, model, &prompt, resolution, references, Some(seed)).await {
        Ok(path) => {
            let path = persist_remote_image(&path, item.id).await?;
            let updated = sqlx::query("UPDATE toonflow.images SET file_path=$2,state='已完成' WHERE id=$1 AND state='生成中'")
                .bind(image_id)
                .bind(&path)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            if updated.rows_affected() == 0 {
                return Err("生成任务已取消".into());
            }
            sqlx::query("UPDATE toonflow.assets SET image_id=$2 WHERE id=$1")
                .bind(item.id)
                .bind(image_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(path)
        }
        Err(reason) => {
            let _ = sqlx::query(
                "UPDATE toonflow.images SET state='生成失败',error_reason=$2 WHERE id=$1",
            )
            .bind(image_id)
            .bind(&reason)
            .execute(pool)
            .await;
            Err(reason)
        }
    }
}

pub(crate) async fn schedule_asset_generation(
    pool: &sqlx::PgPool,
    project_id: i64,
    asset_ids: &[i64],
    concurrent_count: usize,
) -> Result<Vec<Value>, AppError> {
    if asset_ids.is_empty() {
        return Err(AppError::bad_request("ids不能为空"));
    }
    polish_extracted_assets(pool, project_id, asset_ids, concurrent_count)
        .await
        .map_err(AppError::bad_request)?;
    let setting: Option<(Option<i64>, String)> =
        sqlx::query_as("SELECT image_model,image_quality FROM toonflow.projects WHERE id=$1")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|_| AppError::internal("failed to load project image model"))?;
    let (model, resolution) = setting.ok_or_else(|| AppError::not_found("project not found"))?;
    let model = model
        .ok_or_else(|| AppError::bad_request("请先配置项目图片模型"))?
        .to_string();
    let rows: Vec<(i64, String, String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT a.id,a.type,a.name,a.prompt,
                  (SELECT i.file_path FROM toonflow.assets p JOIN toonflow.images i ON i.id=p.image_id WHERE p.id=a.parent_asset_id AND i.state='已完成')
           FROM toonflow.assets a
           WHERE a.project_id=$1 AND a.id=ANY($2) AND coalesce(a.prompt,'')<>''"#,
    )
    .bind(project_id)
    .bind(asset_ids)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to load assets"))?;
    let mut queue = Vec::new();
    for (offset, (id, type_, name, prompt, reference_path)) in rows.into_iter().enumerate() {
        let base64 = match reference_path {
            Some(path) => Some(image_data_url(&path).await.map_err(AppError::bad_request)?),
            None => None,
        };
        let item = ImageItem {
            id,
            type_,
            name,
            prompt,
            base64,
        };
        let seed = toonflow_prompt_store::resolve_seed(pool, Some(project_id), None).await.wrapping_add(offset as i64);
        let image_id = new_image(pool, &item, &model, &resolution, offset as i64, seed).await?;
        queue.push((image_id, item, seed));
    }
    let response = queue
        .iter()
        .map(|(image_id, item, _)| json!({"id":item.id,"imageId":image_id,"state":"生成中"}))
        .collect::<Vec<_>>();
    let pool = pool.clone();
    tokio::spawn(async move {
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrent_count.clamp(1, 10)));
        for (image_id, item, seed) in queue {
            let permit = sem.clone().acquire_owned().await;
            let pool = pool.clone();
            let model = model.clone();
            let resolution = resolution.clone();
            tokio::spawn(async move {
                if permit.is_ok() {
                    let _ =
                        make_image(&pool, project_id, &model, &resolution, item, image_id, seed).await;
                }
            });
        }
    });
    Ok(response)
}

pub(crate) async fn schedule_and_wait_asset_generation(
    pool: &sqlx::PgPool,
    project_id: i64,
    asset_ids: &[i64],
    concurrent_count: usize,
) -> Result<Vec<Value>, AppError> {
    let scheduled =
        schedule_asset_generation(pool, project_id, asset_ids, concurrent_count).await?;
    let image_ids = scheduled
        .iter()
        .filter_map(|item| item.get("imageId").and_then(Value::as_i64))
        .collect::<Vec<_>>();
    if image_ids.len() != asset_ids.len() {
        return Err(AppError::bad_request("部分衍生资产未能创建图片任务"));
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
    loop {
        let rows: Vec<(i64, String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id,state,file_path,error_reason FROM toonflow.images WHERE id=ANY($1) ORDER BY id",
        )
        .bind(&image_ids)
        .fetch_all(pool)
        .await
        .map_err(|_| AppError::internal("failed to wait for derived asset images"))?;
        if rows.len() != image_ids.len() {
            return Err(AppError::bad_request("衍生图片任务记录不完整"));
        }
        let failures = rows
            .iter()
            .filter(|(_, state, _, _)| state == "生成失败" || state == "已取消")
            .map(|(id, _, _, reason)| format!("{id}: {}", reason.as_deref().unwrap_or("生成失败")))
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(AppError::bad_request(format!(
                "衍生图片未全部生成成功：{}",
                failures.join("；")
            )));
        }
        if rows.iter().all(|(_, state, path, _)| {
            state == "已完成" && path.as_deref().is_some_and(|path| !path.is_empty())
        }) {
            return Ok(rows
                .into_iter()
                .map(|(image_id, _, file_path, _)| {
                    json!({"imageId":image_id,"state":"已完成","filePath":file_path})
                })
                .collect());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::bad_request(
                "等待衍生图片生成超时，请检查图片服务",
            ));
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
pub async fn generate_image(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<GenerateImageRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let model = req.model.as_configured();
    let item = ImageItem {
        id: req.id,
        type_: req.type_,
        name: req.name,
        prompt: req.prompt,
        base64: req.base64,
    };
    let seed = toonflow_prompt_store::resolve_seed(&state.pool, Some(req.project_id), None).await;
    let image_id = new_image(&state.pool, &item, &model, &req.resolution, 0, seed).await?;
    match make_image(
        &state.pool,
        req.project_id,
        &model,
        &req.resolution,
        item.clone(),
        image_id,
        seed,
    )
    .await
    {
        Ok(path) => Ok(Json(ApiResponse::new(
            json!({"path":path,"assetsId":item.id}),
        ))),
        Err(reason) => Err(AppError::bad_request(reason)),
    }
}
pub async fn batch_generate_images(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<BatchImageRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let total = req.items.len();
    let model = req.model.as_configured();
    let mut queue = Vec::new();
    let project_seed = toonflow_prompt_store::resolve_seed(&state.pool, Some(req.project_id), None).await;
    for (index, item) in req.items.into_iter().enumerate() {
        let seed = project_seed.wrapping_add(index as i64);
        queue.push((
            new_image(&state.pool, &item, &model, &req.resolution, index as i64, seed).await?,
            item,
            seed,
        ));
    }
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(
            req.concurrent_count.unwrap_or(1).clamp(1, 10),
        ));
        for (image_id, item, seed) in queue {
            let permit = sem.clone().acquire_owned().await;
            let pool = pool.clone();
            let model = model.clone();
            let resolution = req.resolution.clone();
            tokio::spawn(async move {
                if permit.is_ok() {
                    let _ = make_image(&pool, req.project_id, &model, &resolution, item, image_id, seed)
                        .await;
                }
            });
        }
    });
    Ok(Json(ApiResponse::new(json!({"total":total}))))
}
pub async fn poll_images(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<Ids>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows=sqlx::query_as::<_,(i64,String,Option<String>,Option<String>,i64)>("SELECT a.id,i.state,i.file_path,i.error_reason,i.id FROM toonflow.assets a JOIN LATERAL (SELECT id,state,file_path,error_reason FROM toonflow.images WHERE assets_id=a.id ORDER BY id DESC LIMIT 1) i ON true WHERE a.id=ANY($1)").bind(req.ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to poll images"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .map(|r| json!({"id":r.0,"state":r.1,"filePath":r.2,"errorReason":r.3,"imageId":r.4}))
            .collect(),
    )))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetImageRequest {
    assets_id: i64,
}
pub async fn get_images(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<AssetImageRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:read")?;
    let asset: Option<(i64, Option<i64>)> =
        sqlx::query_as("SELECT id,image_id FROM toonflow.assets WHERE id=$1")
            .bind(req.assets_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get asset"))?;
    let asset = asset.ok_or_else(|| AppError::not_found("asset not found"))?;
    let rows=sqlx::query_as::<_,(i64,Option<String>,String)>("SELECT id,file_path,coalesce(state,'') FROM toonflow.images WHERE assets_id=$1 ORDER BY id DESC").bind(req.assets_id).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to get images"))?;
    Ok(Json(ApiResponse::new(
        json!({"id":asset.0,"imageId":asset.1,"tempAssets":rows.into_iter().map(|r|json!({"id":r.0,"filePath":r.1,"state":r.2,"selected":asset.1==Some(r.0)})).collect::<Vec<_>>()}),
    )))
}
#[derive(Deserialize)]
pub struct ImageId {
    id: i64,
}
pub async fn cancel_image(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<ImageId>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let result = sqlx::query("UPDATE toonflow.images SET state='生成失败',error_reason='用户取消生成' WHERE id=$1 AND state='生成中'")
        .bind(req.id)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to cancel image generation"))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"取消成功","canceled":result.rows_affected()>0}),
    )))
}
pub async fn delete_image(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<ImageId>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| AppError::internal("failed to begin transaction"))?;
    sqlx::query("UPDATE toonflow.assets SET image_id=NULL WHERE image_id=$1")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to unbind image"))?;
    sqlx::query("DELETE FROM toonflow.images WHERE id=$1")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::internal("failed to delete image"))?;
    tx.commit()
        .await
        .map_err(|_| AppError::internal("failed to commit transaction"))?;
    Ok(Json(ApiResponse::new(
        json!({"message":"资产图片删除成功"}),
    )))
}
