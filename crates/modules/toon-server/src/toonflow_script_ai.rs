use crate::{
    ToonState, ai_client, shared::require, toonflow_asset_description, toonflow_prompt_store,
};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    script_ids: Vec<i64>,
    project_id: i64,
    group_size: Option<usize>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractedAsset {
    name: String,
    #[serde(default, alias = "description")]
    desc: String,
    #[serde(rename = "type")]
    #[serde(default)]
    type_: String,
    #[serde(default)]
    script_ids: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtractedAppearance {
    role_name: String,
    name: String,
    #[serde(default)]
    scenes: Vec<String>,
    costume_prompt: String,
    #[serde(default)]
    description: String,
    script_id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtractedResult {
    #[serde(default)]
    new_assets: Vec<ExtractedAsset>,
    #[serde(default)]
    existing_asset_refs: Vec<ExtractedAsset>,
    #[serde(default)]
    appearances: Vec<ExtractedAppearance>,
}

fn parse_result(output: &str) -> Result<ExtractedResult, String> {
    let trimmed = output
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(trimmed).or_else(|_| {
        let start = trimmed.find('{').ok_or("AI 未返回 JSON 对象")?;
        let end = trimmed.rfind('}').ok_or("AI 未返回完整 JSON 对象")?;
        serde_json::from_str(&trimmed[start..=end]).map_err(|error| error.to_string())
    })
}

async fn extract_group(
    pool: &sqlx::PgPool,
    project_id: i64,
    script_ids: &[i64],
) -> Result<(), String> {
    let scripts = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id,name,content FROM toonflow.scripts WHERE project_id=$1 AND id=ANY($2)",
    )
    .bind(project_id)
    .bind(script_ids)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if scripts.len() != script_ids.len() {
        return Err("部分剧本不存在".into());
    }
    sqlx::query("UPDATE toonflow.scripts SET extract_state=0,error_reason=NULL WHERE project_id=$1 AND id=ANY($2)")
        .bind(project_id)
        .bind(script_ids)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    let existing = sqlx::query_as::<_, (String, String, String)>(
        r#"SELECT a.name,a.type,a.description FROM toonflow.assets a
           WHERE a.project_id=$1 OR EXISTS(
             SELECT 1 FROM toonflow.project_assets pa
             WHERE pa.project_id=$1 AND pa.asset_id=a.id
           )"#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(|row| format!("{}({})：{}", row.0, row.1, row.2))
    .collect::<Vec<_>>()
    .join("、");
    let content = scripts
        .iter()
        .map(|row| format!("===== 剧本ID:{} {} =====\n{}", row.0, row.1, row.2))
        .collect::<Vec<_>>()
        .join("\n\n");
    let system_prompt = toonflow_prompt_store::load(
        pool,
        "script_asset_extraction",
        "提取 role/scene/tool 基础资产，并逐场输出每个有名角色的 appearances；role 不得包含服装，appearance 必须包含 roleName、name、scenes、costumePrompt、description、scriptId。只输出约定 JSON。",
    )
    .await;
    let output = ai_client::text(
        pool,
        "universalAi",
        &system_prompt,
        &format!("已有资产：{existing}\n\n{content}"),
    )
    .await?;
    let result = parse_result(&output)?;
    let allowed = ["role", "scene", "tool", "costume"];
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM toonflow.script_assets WHERE script_id=ANY($1)")
        .bind(script_ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    for asset in result.new_assets {
        if asset.name.trim().is_empty() || !allowed.contains(&asset.type_.as_str()) {
            continue;
        }
        if asset.type_ == "role" {
            toonflow_asset_description::validate_role_description(&asset.desc)?;
        }
        let existing_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM toonflow.assets WHERE project_id=$1 AND name=$2 AND type=$3 LIMIT 1",
        )
        .bind(project_id)
        .bind(asset.name.trim())
        .bind(&asset.type_)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        let asset_id = if let Some(id) = existing_id {
            id
        } else {
            let id = chrono::Utc::now().timestamp_micros();
            sqlx::query("INSERT INTO toonflow.assets(id,name,type,description,project_id,start_time) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(id).bind(asset.name.trim()).bind(&asset.type_).bind(asset.desc.trim()).bind(project_id).bind(chrono::Utc::now().timestamp_millis())
                .execute(&mut *tx).await.map_err(|error|error.to_string())?;
            id
        };
        sqlx::query("INSERT INTO toonflow.project_assets(project_id,asset_id,linked_at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(project_id).bind(asset_id).bind(chrono::Utc::now().timestamp_millis())
            .execute(&mut *tx).await.map_err(|error|error.to_string())?;
        for script_id in asset
            .script_ids
            .into_iter()
            .filter(|id| script_ids.contains(id))
        {
            sqlx::query("INSERT INTO toonflow.script_assets(script_id,asset_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(script_id).bind(asset_id).execute(&mut *tx).await.map_err(|error|error.to_string())?;
        }
    }
    for reference in result.existing_asset_refs {
        let asset_id: Option<i64> = sqlx::query_scalar(
            r#"SELECT a.id FROM toonflow.assets a
               WHERE a.name=$2 AND a.type=$3 AND (
                 a.project_id=$1 OR EXISTS(
                   SELECT 1 FROM toonflow.project_assets pa
                   WHERE pa.project_id=$1 AND pa.asset_id=a.id
                 )
               ) ORDER BY CASE WHEN a.project_id=$1 THEN 0 ELSE 1 END LIMIT 1"#,
        )
        .bind(project_id)
        .bind(reference.name.trim())
        .bind(&reference.type_)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if let Some(asset_id) = asset_id {
            if reference.type_ == "role" && !reference.desc.trim().is_empty() {
                toonflow_asset_description::validate_role_description(&reference.desc)?;
                sqlx::query("UPDATE toonflow.assets SET description=$1 WHERE id=$2")
                    .bind(reference.desc.trim())
                    .bind(asset_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            for script_id in reference
                .script_ids
                .into_iter()
                .filter(|id| script_ids.contains(id))
            {
                sqlx::query("INSERT INTO toonflow.script_assets(script_id,asset_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                    .bind(script_id).bind(asset_id).execute(&mut *tx).await.map_err(|error|error.to_string())?;
            }
        }
    }
    let mut retained_appearance_ids = Vec::new();
    for (offset, appearance) in result.appearances.into_iter().enumerate() {
        if !script_ids.contains(&appearance.script_id)
            || appearance.role_name.trim().is_empty()
            || appearance.name.trim().is_empty()
            || appearance.costume_prompt.trim().is_empty()
        {
            continue;
        }
        let role_asset_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM toonflow.assets WHERE project_id=$1 AND type='role' AND name=$2 AND parent_asset_id IS NULL LIMIT 1",
        )
        .bind(project_id)
        .bind(appearance.role_name.trim())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        let Some(role_asset_id) = role_asset_id else {
            continue;
        };
        let id = chrono::Utc::now().timestamp_micros() + offset as i64;
        let retained_id: i64 = sqlx::query_scalar("INSERT INTO toonflow.character_appearances(id,project_id,script_id,role_asset_id,name,scenes,costume_prompt,description,create_time,update_time) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9) ON CONFLICT(script_id,role_asset_id,name) DO UPDATE SET scenes=excluded.scenes,costume_prompt=excluded.costume_prompt,description=excluded.description,update_time=excluded.update_time RETURNING id")
            .bind(id)
            .bind(project_id)
            .bind(appearance.script_id)
            .bind(role_asset_id)
            .bind(appearance.name.trim())
            .bind(json!(appearance.scenes))
            .bind(appearance.costume_prompt.trim())
            .bind(appearance.description.trim())
            .bind(chrono::Utc::now().timestamp_millis())
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        retained_appearance_ids.push(retained_id);
    }
    sqlx::query(
        "DELETE FROM toonflow.character_appearances WHERE script_id=ANY($1) AND NOT(id=ANY($2))",
    )
    .bind(script_ids)
    .bind(&retained_appearance_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE toonflow.scripts SET extract_state=1,error_reason=NULL WHERE project_id=$1 AND id=ANY($2)")
        .bind(project_id).bind(script_ids).execute(&mut *tx).await.map_err(|error|error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn extract_assets(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ExtractRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    if request.script_ids.is_empty() {
        return Err(AppError::bad_request("请先选择剧本"));
    }
    sqlx::query("UPDATE toonflow.scripts SET extract_state=2,error_reason=NULL WHERE project_id=$1 AND id=ANY($2)")
        .bind(request.project_id).bind(&request.script_ids).execute(&state.pool).await.map_err(|_|AppError::internal("failed to queue asset extraction"))?;
    let pool = state.pool.clone();
    let project_id = request.project_id;
    let ids = request.script_ids;
    let group_size = request.group_size.unwrap_or(5).clamp(1, 20);
    tokio::spawn(async move {
        for group in ids.chunks(group_size) {
            if let Err(reason) = extract_group(&pool, project_id, group).await {
                let _=sqlx::query("UPDATE toonflow.scripts SET extract_state=-1,error_reason=$3 WHERE project_id=$1 AND id=ANY($2)").bind(project_id).bind(group).bind(&reason).execute(&pool).await;
            }
        }
    });
    Ok(Json(ApiResponse::new(json!({"message":"开始提取资产"}))))
}

#[derive(Deserialize)]
pub struct Ids {
    ids: Vec<i64>,
}

pub async fn poll(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<Ids>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows = sqlx::query_as::<_, (i64, Option<i32>, Option<String>, i64)>(
        "SELECT s.id,s.extract_state,s.error_reason,(SELECT count(*) FROM toonflow.character_appearances ca WHERE ca.script_id=s.id) FROM toonflow.scripts s WHERE s.id=ANY($1) AND coalesce(s.extract_state,2)<>0",
    )
    .bind(request.ids)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::internal("failed to poll script assets"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .map(|row| json!({"id":row.0,"extractState":row.1,"errorReason":row.2,"appearanceCount":row.3}))
            .collect(),
    )))
}
