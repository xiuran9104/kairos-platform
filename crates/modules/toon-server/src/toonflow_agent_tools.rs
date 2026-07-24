use crate::{
    ToonState, ai_client, shared::require, toonflow_agent_runtime, toonflow_agents,
    toonflow_asset_ai, toonflow_image_workflow, toonflow_ws::WsEmitter,
};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// Parse event JSON array into readable text
fn format_events(events_json: &str) -> String {
    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(events_json) {
        arr.iter()
            .enumerate()
            .map(|(i, v)| {
                let name = v.get("name").and_then(Value::as_str).unwrap_or("");
                let detail = v.get("detail").and_then(Value::as_str).unwrap_or("");
                format!("  {}. {}：{}", i + 1, name, detail)
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        events_json.to_string()
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn tagged(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

async fn role_names_without_appearances(
    pool: &sqlx::PgPool,
    project_id: i64,
    script_id: i64,
) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar(
        r#"SELECT a.name
           FROM toonflow.script_assets sa
           JOIN toonflow.assets a ON a.id=sa.asset_id
           WHERE sa.script_id=$1 AND a.project_id=$2 AND a.type='role'
             AND a.parent_asset_id IS NULL
             AND NOT EXISTS (SELECT 1 FROM toonflow.character_appearances ca WHERE ca.script_id=$1 AND ca.role_asset_id=a.id)
           ORDER BY a.id"#,
    )
    .bind(script_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to validate extracted character appearances"))
}

async fn missing_appearance_derivatives(
    pool: &sqlx::PgPool,
    project_id: i64,
    script_id: i64,
) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar(
        r#"SELECT a.name || ' / ' || ca.name
           FROM toonflow.character_appearances ca
           JOIN toonflow.assets a ON a.id=ca.role_asset_id
           WHERE ca.script_id=$1 AND ca.project_id=$2
             AND NOT EXISTS (
               SELECT 1 FROM toonflow.assets d
               WHERE d.appearance_id=ca.id AND d.parent_asset_id=ca.role_asset_id
             )
           ORDER BY a.id,ca.id"#,
    )
    .bind(script_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to validate appearance derivatives"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAgentRequest {
    project_id: i64,
    agent_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePlanRequest {
    project_id: i64,
    agent_type: String,
    data: Value,
}

#[derive(Deserialize)]
pub struct UpdatePlanRequest {
    id: i64,
    data: Value,
}

fn validate_script_agent(agent: &str) -> Result<(), AppError> {
    if agent == "scriptAgent" {
        Ok(())
    } else {
        Err(AppError::bad_request("agentType 必须是 scriptAgent"))
    }
}

async fn scripts(pool: &sqlx::PgPool, project_id: i64) -> Result<Value, AppError> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id,name,content FROM toonflow.scripts WHERE project_id=$1 ORDER BY create_time,id",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|_| AppError::internal("failed to load scripts"))?;
    Ok(json!(
        rows.into_iter()
            .map(|(id, name, content)| json!({"id":id,"name":name,"content":content}))
            .collect::<Vec<_>>()
    ))
}

pub async fn get_plan(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ProjectAgentRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:read")?;
    validate_script_agent(&request.agent_type)?;
    let row:Option<(i64,Value)>=sqlx::query_as("SELECT id,data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id IS NULL AND key=$2").bind(request.project_id).bind(&request.agent_type).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to load plan data"))?;
    let (id, mut data) = if let Some(row) = row {
        row
    } else {
        let id: i64=sqlx::query_scalar("INSERT INTO toonflow.agent_work_data(project_id,episodes_id,key,data,create_time,update_time)VALUES($1,NULL,$2,$3,$4,$4) RETURNING id").bind(request.project_id).bind(&request.agent_type).bind(json!({"storySkeleton":"","adaptationStrategy":""})).bind(now_ms()).fetch_one(&state.pool).await.map_err(|_|AppError::internal("failed to create plan data"))?;
        (id, json!({"storySkeleton":"","adaptationStrategy":""}))
    };
    data["script"] = scripts(&state.pool, request.project_id).await?;
    Ok(Json(ApiResponse::new(json!({"id":id,"data":data}))))
}

pub async fn set_plan(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SavePlanRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    validate_script_agent(&request.agent_type)?;
    let skeleton = request
        .data
        .get("storySkeleton")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let strategy = request
        .data
        .get("adaptationStrategy")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id:i64=sqlx::query_scalar("INSERT INTO toonflow.agent_work_data(project_id,episodes_id,key,data,create_time,update_time)VALUES($1,NULL,$2,$3,$4,$4) ON CONFLICT(project_id,key) WHERE episodes_id IS NULL DO UPDATE SET data=excluded.data,update_time=excluded.update_time RETURNING id").bind(request.project_id).bind(&request.agent_type).bind(json!({"storySkeleton":skeleton,"adaptationStrategy":strategy})).bind(now_ms()).fetch_one(&state.pool).await.map_err(|_|AppError::internal("failed to save plan data"))?;
    if let Some(items) = request.data.get("script").and_then(Value::as_array) {
        for (index, item) in items.iter().enumerate() {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("未命名剧本");
            let content = item
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let script_id = now_ms() * 1000 + index as i64;
            sqlx::query("INSERT INTO toonflow.scripts(id,name,content,project_id,create_time)VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,content=excluded.content").bind(item.get("id").and_then(Value::as_i64).unwrap_or(script_id)).bind(name).bind(content).bind(request.project_id).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to save agent script"))?;
        }
    }
    Ok(Json(ApiResponse::new(json!({"id":id}))))
}

pub async fn update_plan(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<UpdatePlanRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT project_id,key FROM toonflow.agent_work_data WHERE id=$1")
            .bind(request.id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to load plan data"))?;
    let (project_id, key) = row.ok_or_else(|| AppError::not_found("plan data not found"))?;
    validate_script_agent(&key)?;
    sqlx::query("UPDATE toonflow.agent_work_data SET data=$2,update_time=$3 WHERE id=$1")
        .bind(request.id)
        .bind(&request.data)
        .bind(now_ms())
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to update plan data"))?;
    if let Some(items) = request.data.get("script").and_then(Value::as_array) {
        for item in items {
            if let (Some(id), Some(content)) = (
                item.get("id").and_then(Value::as_i64),
                item.get("content").and_then(Value::as_str),
            ) {
                sqlx::query("UPDATE toonflow.scripts SET content=$3 WHERE id=$1 AND project_id=$2")
                    .bind(id)
                    .bind(project_id)
                    .bind(content)
                    .execute(&state.pool)
                    .await
                    .map_err(|_| AppError::internal("failed to update script"))?;
            }
        }
    }
    Ok(Json(ApiResponse::new(json!(true))))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolRequest {
    pub(crate) agent_type: String,
    pub(crate) project_id: i64,
    pub(crate) script_id: Option<i64>,
    pub(crate) tool_name: String,
    #[serde(default)]
    pub(crate) arguments: Value,
    #[serde(skip, default)]
    pub(crate) emitter: Option<WsEmitter>,
}

pub async fn execute(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ToolRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    let (call_id, value) = execute_recorded(&state, &request).await?;
    Ok(Json(ApiResponse::new(
        json!({"callId":call_id,"result":value}),
    )))
}

pub(crate) async fn execute_recorded(
    state: &ToonState,
    request: &ToolRequest,
) -> Result<(i64, Value), AppError> {
    let call_id = now_ms() * 1000;
    sqlx::query("INSERT INTO toonflow.agent_tool_calls(id,agent_type,tool_name,arguments,state,create_time)VALUES($1,$2,$3,$4,'running',$5)").bind(call_id).bind(&request.agent_type).bind(&request.tool_name).bind(&request.arguments).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to start tool call"))?;
    match execute_inner(state, request).await {
        Ok(value) => {
            sqlx::query("UPDATE toonflow.agent_tool_calls SET result=$2,state='success',finish_time=$3 WHERE id=$1").bind(call_id).bind(&value).bind(now_ms()).execute(&state.pool).await.ok();
            // UI-triggered production tools (for example an image-edit node) must join the
            // same per-script memory as the persistent Production Agent conversation.
            if request.emitter.is_none() && request.agent_type == "productionAgent" {
                if let Some(script_id) = request.script_id {
                    let memory_id = call_id + 1;
                    let isolation_key =
                        format!("productionAgent:{}:{}", request.project_id, script_id);
                    let content = format!(
                        "工具 {} 已执行。参数：{}。结果：{}",
                        request.tool_name, request.arguments, value
                    );
                    if sqlx::query("INSERT INTO toonflow.agent_memories(id,agent_type,isolation_key,role,content,create_time)VALUES($1,'productionAgent',$2,'assistant:execution',$3,$4)")
                        .bind(memory_id).bind(isolation_key).bind(&content).bind(now_ms()).execute(&state.pool).await.is_ok()
                    {
                        toonflow_agent_runtime::store_memory_embedding(&state.pool, memory_id, &content).await;
                    }
                }
            }
            Ok((call_id, value))
        }
        Err(error) => {
            sqlx::query("UPDATE toonflow.agent_tool_calls SET state='failed',error_reason=$2,finish_time=$3 WHERE id=$1").bind(call_id).bind(format!("{error:?}")).bind(now_ms()).execute(&state.pool).await.ok();
            Err(error)
        }
    }
}

/// Execute a tool with WebSocket emitter for real-time visualization.
/// Falls back to regular execute_recorded when emitter is not available.
pub(crate) async fn execute_with_emitter(
    state: &ToonState,
    request: &ToolRequest,
    _emitter: &WsEmitter,
    _msg_id: &str,
) -> Result<(i64, Value), AppError> {
    execute_recorded(state, request).await
}

/// Execute a read-only tool for sub-agents
async fn execute_sub_tool(state: &ToonState, project_id: i64, name: &str, args: &Value) -> String {
    match name {
        "get_novel_events" => {
            let indexes: Vec<i32> = args
                .get("chapterIndexs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_i64().map(|x| x as i32))
                        .collect()
                })
                .unwrap_or_default();
            let rows: Vec<(i32, String, Option<String>)> = sqlx::query_as(
                "SELECT chapter_index,chapter,event FROM toonflow.novels WHERE project_id=$1 AND chapter_index=ANY($2) ORDER BY chapter_index",
            )
            .bind(project_id).bind(&indexes)
            .fetch_all(&state.pool).await.unwrap_or_default();
            rows.into_iter()
                .map(|(i, c, e)| {
                    format!("第{i}章「{c}」\n{}", format_events(&e.unwrap_or_default()))
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        "get_novel_text" => {
            let index: i32 = args
                .get("chapterIndex")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            sqlx::query_scalar(
                "SELECT chapter_data FROM toonflow.novels WHERE project_id=$1 AND chapter_index=$2",
            )
            .bind(project_id)
            .bind(index)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or_default()
            .unwrap_or_default()
        }
        "get_planData" => {
            let data: Option<Value> = sqlx::query_scalar(
                "SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id IS NULL AND key='scriptAgent'",
            )
            .bind(project_id).fetch_optional(&state.pool).await.unwrap_or_default();
            let key = args.get("key").and_then(Value::as_str).unwrap_or("");
            if key.is_empty() {
                // Return all data with clear labels
                let d = data.as_ref();
                let skeleton = d
                    .and_then(|v| v.get("storySkeleton"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("（空）");
                let adaptation = d
                    .and_then(|v| v.get("adaptationStrategy"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("（空）");
                format!(
                    "可用 key: storySkeleton, adaptationStrategy\n\n=== storySkeleton ===\n{skeleton}\n\n=== adaptationStrategy ===\n{adaptation}"
                )
            } else {
                // Try exact match first, then case-insensitive
                let result = data
                    .as_ref()
                    .and_then(|d| d.get(key))
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                if result.is_some() {
                    return result.unwrap();
                }
                // Try known key mappings
                let mapped = match key.to_lowercase().as_str() {
                    "story_skeleton" | "skeleton" => data
                        .as_ref()
                        .and_then(|d| d.get("storySkeleton"))
                        .and_then(|v| v.as_str()),
                    "adaptation_strategy" | "adaptation" => data
                        .as_ref()
                        .and_then(|d| d.get("adaptationStrategy"))
                        .and_then(|v| v.as_str()),
                    "script" => data
                        .as_ref()
                        .and_then(|d| d.get("script"))
                        .and_then(|v| v.as_str()),
                    _ => None,
                };
                mapped.map(|s| s.to_string()).unwrap_or_else(|| {
                    format!("key '{key}' 不存在。可用 key: storySkeleton, adaptationStrategy")
                })
            }
        }
        "get_script_content" => {
            let ids: Vec<i64> = args
                .get("ids")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
                .unwrap_or_default();
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT name,content FROM toonflow.scripts WHERE project_id=$1 AND id=ANY($2)",
            )
            .bind(project_id)
            .bind(&ids)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
            rows.into_iter()
                .map(|(n, c)| format!("<scriptItem name=\"{n}\">{c}</scriptItem>"))
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => format!("未知工具: {name}"),
    }
}

pub(crate) async fn execute_inner(
    state: &ToonState,
    request: &ToolRequest,
) -> Result<Value, AppError> {
    if request.tool_name == "use_skill" {
        let path = request
            .arguments
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::bad_request("缺少 Skill path"))?;
        // Support any dynamic skill: production_skills/, art_skills/, story_skills/
        if !path.contains('/')
            || path.starts_with("script_")
            || path.starts_with("production_agent")
        {
            return Err(AppError::bad_request(
                "只能加载动态 Skill（production_skills/、art_skills/ 等路径）",
            ));
        }
        let content = toonflow_agent_runtime::load_skill(&state.pool, path)
            .await
            .map_err(AppError::bad_request)?;
        return Ok(json!({"path":path,"content":content}));
    }
    match (request.agent_type.as_str(), request.tool_name.as_str()) {
        ("scriptAgent", "deepRetrieve") => {
            let keyword = request
                .arguments
                .get("keyword")
                .and_then(Value::as_str)
                .unwrap_or("");
            let agent_type = &request.agent_type;
            // Build isolation key from project ID (matches WebSocket isolation pattern)
            let isolation_key = format!("{agent_type}:{}:project", request.project_id);
            let mems = toonflow_agent_runtime::relevant_memories(
                &state.pool,
                agent_type,
                &isolation_key,
                keyword,
                5,
            )
            .await
            .map_err(|_| AppError::internal("deepRetrieve 失败"))?;
            Ok(json!(mems.join("\n\n")))
        }
        ("scriptAgent", "get_novel_events") => {
            let indexes = request
                .arguments
                .get("chapterIndexs")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|v| v.as_i64())
                .collect::<Vec<_>>();
            let rows:Vec<(i32,String,Option<String>)>=sqlx::query_as("SELECT chapter_index,chapter,event FROM toonflow.novels WHERE project_id=$1 AND chapter_index=ANY($2) ORDER BY chapter_index").bind(request.project_id).bind(indexes.iter().map(|v|*v as i32).collect::<Vec<_>>()).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to get novel events"))?;
            Ok(json!(
                rows.into_iter()
                    .map(|(i, c, e)| format!(
                        "第{i}章「{c}」\n{}",
                        format_events(&e.unwrap_or_default())
                    ))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            ))
        }
        ("scriptAgent", "get_novel_text") => {
            let index = request
                .arguments
                .get("chapterIndex")
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                .ok_or_else(|| AppError::bad_request("缺少 chapterIndex"))?;
            let text: Option<String> = sqlx::query_scalar(
                "SELECT chapter_data FROM toonflow.novels WHERE project_id=$1 AND chapter_index=$2",
            )
            .bind(request.project_id)
            .bind(index as i32)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get novel text"))?;
            Ok(json!(text.unwrap_or_default()))
        }
        ("scriptAgent", "get_script_content") => {
            let ids = request
                .arguments
                .get("ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                .collect::<Vec<_>>();
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT name,content FROM toonflow.scripts WHERE project_id=$1 AND id=ANY($2)",
            )
            .bind(request.project_id)
            .bind(ids)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get scripts"))?;
            Ok(json!(
                rows.into_iter()
                    .map(|(n, c)| format!("<scriptItem name=\"{n}\">{c}</scriptItem>"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
        ("scriptAgent", "save_scripts") => {
            let scripts = request
                .arguments
                .get("scripts")
                .and_then(Value::as_array)
                .ok_or_else(|| AppError::bad_request("缺少 scripts"))?;
            if scripts.is_empty() {
                return Err(AppError::bad_request("剧本列表不能为空"));
            }
            let mut tx = state
                .pool
                .begin()
                .await
                .map_err(|_| AppError::internal("failed to begin script save"))?;
            let timestamp = now_ms();
            let mut saved = Vec::with_capacity(scripts.len());
            for (index, script) in scripts.iter().enumerate() {
                let name = script
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();
                let content = script
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();
                if name.is_empty() || content.is_empty() {
                    return Err(AppError::bad_request("剧本名称和内容不能为空"));
                }
                let id = timestamp * 1000 + index as i64;
                sqlx::query("INSERT INTO toonflow.scripts(id,name,content,project_id,create_time) VALUES($1,$2,$3,$4,$5)")
                    .bind(id)
                    .bind(name)
                    .bind(content)
                    .bind(request.project_id)
                    .bind(timestamp)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AppError::internal("failed to save generated script"))?;
                saved.push(json!({"id":id,"name":name}));
            }
            tx.commit()
                .await
                .map_err(|_| AppError::internal("failed to commit generated scripts"))?;
            Ok(json!({"saved":saved}))
        }
        ("scriptAgent", name)
            if name.starts_with("run_sub_agent_") || name == "run_supervision_agent" =>
        {
            let (agent_key, label, tag) = match name {
                "run_sub_agent_storySkeleton" => (
                    "scriptAgent:storySkeletonAgent",
                    "故事骨架",
                    Some("storySkeleton"),
                ),
                "run_sub_agent_adaptationStrategy" => (
                    "scriptAgent:adaptationStrategyAgent",
                    "改编策略",
                    Some("adaptationStrategy"),
                ),
                "run_sub_agent_script" => ("scriptAgent:scriptAgent", "剧本", None),
                "run_supervision_agent" => ("scriptAgent:supervisionAgent", "监督", None),
                _ => return Err(AppError::bad_request("不支持的剧本子 Agent")),
            };
            let prompt = request
                .arguments
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 prompt"))?;
            let system = toonflow_agent_runtime::load_agent_skill(&state.pool, agent_key)
                .await
                .map_err(AppError::bad_request)?;

            // Create sub-agent message bubble if emitter is available
            let sub_msg = if let Some(ref emitter) = request.emitter {
                let sub_label = match label {
                    "监督" => "编辑",
                    _ => "编剧",
                };
                let (mid, _) = emitter.new_message(sub_label, "assistant");
                let cid = emitter.add_content(&mid, "text", &json!(""));
                Some((mid, cid, emitter.clone()))
            } else {
                None
            };

            // Build project context
            let project_hint: String = sqlx::query_scalar(
                "SELECT '作品名：'||name||'\n小说类型：'||type||'\n小说简介：'||COALESCE(intro,'无')||'\n视觉风格：'||COALESCE(art_style,'未设置')||'\n视频画幅：'||COALESCE(video_ratio,'16:9') FROM toonflow.projects WHERE id=$1",
            )
            .bind(request.project_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::bad_request("无法加载项目信息"))?
            .unwrap_or_default();
            let full_system = format!(
                "{system}\n\n## 当前项目\n{project_hint}\n\n你是 Toonflow 的{label}子 Agent。请使用工具读取所需数据，然后完成任务并输出要求的 XML 格式内容。"
            );

            // Sub-agent read-only tool definitions
            let sub_tools: Vec<Value> = vec![
                json!({"type":"function","function":{"name":"get_novel_events","description":"获取项目章节事件列表","parameters":{"type":"object","properties":{"chapterIndexs":{"type":"array","items":{"type":"number"},"description":"章节编号列表"}},"required":["chapterIndexs"]}}}),
                json!({"type":"function","function":{"name":"get_novel_text","description":"获取指定章节的原始文本","parameters":{"type":"object","properties":{"chapterIndex":{"type":"number","description":"章节编号"}},"required":["chapterIndex"]}}}),
                json!({"type":"function","function":{"name":"get_planData","description":"读取工作区已有数据（故事骨架、改编策略等）","parameters":{"type":"object","properties":{"key":{"type":"string","description":"数据key"}},"required":["key"]}}}),
                json!({"type":"function","function":{"name":"get_script_content","description":"读取已有剧本内容","parameters":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"number"},"description":"剧本ID列表"}},"required":["ids"]}}}),
            ];

            // Run sub-agent with native function calling
            let mut messages = vec![
                json!({"role":"system","content":full_system}),
                json!({"role":"user","content":prompt}),
            ];
            let mut output = String::new();
            for _round in 0..8 {
                let raw = ai_client::text_tools(
                    &state.pool,
                    agent_key,
                    messages.clone(),
                    sub_tools.clone(),
                )
                .await
                .map_err(AppError::bad_request)?;
                let message = raw
                    .pointer("/choices/0/message")
                    .cloned()
                    .ok_or_else(|| AppError::bad_request("模型响应缺少 message"))?;
                let calls = message
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if calls.is_empty() {
                    output = message
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if output.is_empty() {
                        return Err(AppError::bad_request("子 Agent 未返回有效内容"));
                    }
                    break;
                }
                messages.push(message);
                for call in &calls {
                    let call_id = call.get("id").and_then(Value::as_str).unwrap_or("");
                    let tool_name = call
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let args = call
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}");
                    let args: Value = serde_json::from_str(args).unwrap_or(json!({}));
                    let result =
                        execute_sub_tool(state, request.project_id, tool_name, &args).await;
                    messages.push(json!({"role":"tool","tool_call_id":call_id,"content":result}));
                }
            }
            if output.is_empty() {
                if let Some((mid, _, emitter)) = &sub_msg {
                    emitter.update_message(mid, "error", Some("子 Agent 工具调用超过最大轮数"));
                }
                return Err(AppError::bad_request("子 Agent 工具调用超过最大轮数"));
            }

            // Complete sub-agent message bubble
            if let Some((mid, cid, emitter)) = &sub_msg {
                emitter.text_delta(mid, cid, &output);
                emitter.text_complete(mid, cid);
                emitter.update_message(mid, "complete", None);
            }
            if let Some(tag) = tag {
                if let Some(content) = tagged(&output, tag) {
                    let mut data:Value=sqlx::query_scalar("SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id IS NULL AND key='scriptAgent'").bind(request.project_id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to load script workspace"))?.unwrap_or_else(||json!({"storySkeleton":"","adaptationStrategy":""}));
                    data[tag] = json!(content);
                    sqlx::query("INSERT INTO toonflow.agent_work_data(project_id,episodes_id,key,data,create_time,update_time)VALUES($1,NULL,'scriptAgent',$2,$3,$3) ON CONFLICT(project_id,key) WHERE episodes_id IS NULL DO UPDATE SET data=excluded.data,update_time=excluded.update_time").bind(request.project_id).bind(data).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to save sub agent result"))?;
                }
            }
            Ok(json!({"agent":agent_key,"content":output}))
        }
        ("productionAgent", "get_flowData") => {
            let script_id = request
                .script_id
                .ok_or_else(|| AppError::bad_request("生产工具缺少 scriptId"))?;
            let key = request
                .arguments
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if matches!(key, "script" | "assets") {
                let (script, assets) = crate::toonflow_asset_context::load_script_context(
                    &state.pool,
                    request.project_id,
                    script_id,
                )
                .await
                .map_err(|_| AppError::internal("failed to build production asset context"))?;
                return Ok(if key == "script" {
                    json!(script)
                } else {
                    assets
                });
            }
            let data:Option<Value>=sqlx::query_scalar("SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id=$2 AND key='productionAgent'").bind(request.project_id).bind(script_id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to get flow data"))?;
            let data = data.unwrap_or_else(|| json!({}));
            Ok(if key.is_empty() {
                data
            } else {
                data.get(key).cloned().unwrap_or(Value::Null)
            })
        }
        ("productionAgent", "get_video_workbench") => {
            let script_id = request
                .script_id
                .ok_or_else(|| AppError::bad_request("视频工具缺少 scriptId"))?;
            crate::toonflow_video::load_generate_data(&state.pool, request.project_id, script_id)
                .await
        }
        ("productionAgent", "generate_video_prompt") => {
            let track_id = request
                .arguments
                .get("trackId")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 trackId"))?;
            let setting: Option<(Option<i64>, String)> =
                sqlx::query_as("SELECT video_model,mode FROM toonflow.projects WHERE id=$1")
                    .bind(request.project_id)
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(|_| AppError::internal("failed to load video settings"))?;
            let (model, mode) = setting.ok_or_else(|| AppError::not_found("project not found"))?;
            let model = model.ok_or_else(|| AppError::bad_request("项目未配置视频模型"))?;
            let prompt = crate::toonflow_video::create_prompt(
                &state.pool,
                track_id,
                request.project_id,
                &model.to_string(),
                &mode,
            )
            .await
            .map_err(AppError::bad_request)?;
            Ok(json!({"trackId":track_id,"prompt":prompt}))
        }
        ("productionAgent", "update_video_prompt") => {
            let track_id = request
                .arguments
                .get("trackId")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 trackId"))?;
            let prompt = request
                .arguments
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 prompt"))?;
            let result = sqlx::query(
                "UPDATE toonflow.video_tracks SET prompt=$3 WHERE id=$1 AND project_id=$2",
            )
            .bind(track_id)
            .bind(request.project_id)
            .bind(prompt)
            .execute(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to update video prompt"))?;
            if result.rows_affected() == 0 {
                return Err(AppError::not_found("video track not found"));
            }
            Ok(json!({"trackId":track_id,"prompt":prompt}))
        }
        ("productionAgent", "select_video") => {
            let track_id = request
                .arguments
                .get("trackId")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 trackId"))?;
            let video_id = request
                .arguments
                .get("videoId")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 videoId"))?;
            let result=sqlx::query("UPDATE toonflow.video_tracks t SET video_id=$3 WHERE t.id=$1 AND t.project_id=$2 AND EXISTS(SELECT 1 FROM toonflow.videos v WHERE v.id=$3 AND v.video_track_id=t.id AND v.state='生成成功')").bind(track_id).bind(request.project_id).bind(video_id).execute(&state.pool).await.map_err(|_|AppError::internal("failed to select video"))?;
            if result.rows_affected() == 0 {
                return Err(AppError::bad_request("轨道或成功视频不存在"));
            }
            Ok(json!({"trackId":track_id,"videoId":video_id}))
        }
        ("productionAgent", "add_deriveAsset") => {
            let parent = request
                .arguments
                .get("assetsId")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 assetsId"))?;
            let appearance_id = request
                .arguments
                .get("appearanceId")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 appearanceId，禁止临时编写服装"))?;
            let script_id = request
                .script_id
                .ok_or_else(|| AppError::bad_request("人物造型写入缺少 scriptId"))?;
            let appearance: Option<(String, String)> = sqlx::query_as(
                "SELECT name,costume_prompt FROM toonflow.character_appearances WHERE id=$1 AND project_id=$2 AND script_id=$3 AND role_asset_id=$4",
            )
            .bind(appearance_id)
            .bind(request.project_id)
            .bind(script_id)
            .bind(parent)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get character appearance"))?;
            let (appearance_name, costume_prompt) = appearance.ok_or_else(|| {
                AppError::bad_request("appearanceId 与当前剧本人物不匹配，请重新读取 assets")
            })?;
            let parent_type: Option<String> = sqlx::query_scalar(
                "SELECT type FROM toonflow.assets WHERE id=$1 AND project_id=$2",
            )
            .bind(parent)
            .bind(request.project_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to get parent asset"))?;
            let existing_id: Option<i64> = sqlx::query_scalar(
                r#"SELECT id FROM toonflow.assets
                   WHERE project_id=$1 AND parent_asset_id=$3
                     AND (appearance_id=$2 OR (
                       appearance_id IS NULL AND name=$4 AND description=$5
                     ))
                   ORDER BY appearance_id NULLS LAST LIMIT 1"#,
            )
            .bind(request.project_id)
            .bind(appearance_id)
            .bind(parent)
            .bind(&appearance_name)
            .bind(&costume_prompt)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to find appearance derivative"))?;
            let id = existing_id.unwrap_or_else(|| {
                request
                    .arguments
                    .get("id")
                    .and_then(Value::as_i64)
                    .unwrap_or(now_ms() * 1000)
            });
            let parent_type =
                parent_type.ok_or_else(|| AppError::not_found("parent asset not found"))?;
            if parent_type != "role" {
                return Err(AppError::bad_request(
                    "只有人物资产需要创建衍生图；场景变化请在分镜阶段生成",
                ));
            }
            sqlx::query("INSERT INTO toonflow.assets(id,name,prompt,prompt_state,type,description,parent_asset_id,appearance_id,project_id,start_time)VALUES($1,$2,$3,'已完成',$4,$3,$5,$6,$7,$8) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,prompt=excluded.prompt,prompt_state='已完成',prompt_error_reason=NULL,appearance_id=excluded.appearance_id,image_id=CASE WHEN toonflow.assets.description IS DISTINCT FROM excluded.description THEN NULL ELSE toonflow.assets.image_id END")
                .bind(id).bind(&appearance_name).bind(&costume_prompt).bind(parent_type).bind(parent).bind(appearance_id).bind(request.project_id).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to save derived asset"))?;
            sqlx::query("INSERT INTO toonflow.script_assets(script_id,asset_id)VALUES($1,$2) ON CONFLICT DO NOTHING").bind(script_id).bind(id).execute(&state.pool).await.ok();
            Ok(json!({"id":id,"appearanceId":appearance_id,"name":appearance_name}))
        }
        ("productionAgent", "del_deriveAsset") => {
            let id = request
                .arguments
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 id"))?;
            sqlx::query("DELETE FROM toonflow.assets WHERE id=$1 AND project_id=$2 AND parent_asset_id IS NOT NULL").bind(id).bind(request.project_id).execute(&state.pool).await.map_err(|_|AppError::internal("failed to delete derived asset"))?;
            Ok(json!(true))
        }
        ("productionAgent", "generate_deriveAsset") => {
            let ids = request
                .arguments
                .get("ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_i64())
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return Err(AppError::bad_request("ids不能为空"));
            }
            let valid_ids: Vec<i64> = sqlx::query_scalar(
                "SELECT id FROM toonflow.assets WHERE project_id=$1 AND id=ANY($2) AND type='role' AND parent_asset_id IS NOT NULL",
            )
            .bind(request.project_id)
            .bind(&ids)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to validate derived assets"))?;
            if valid_ids.len() != ids.len() {
                return Err(AppError::bad_request(
                    "generate_deriveAsset 只能生成有父资产的人物衍生图，禁止传入人物/场景/道具基础资产",
                ));
            }
            let rows = toonflow_asset_ai::schedule_and_wait_asset_generation(
                &state.pool,
                request.project_id,
                &valid_ids,
                request
                    .arguments
                    .get("concurrentCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(3) as usize,
            )
            .await?;
            Ok(json!(rows))
        }
        ("productionAgent", name) if name.starts_with("run_sub_agent_") => {
            let (agent_key, label, flow_tag, allowed_tools): (_, _, _, &[&str]) = match name {
                "run_sub_agent_derive_assets" => (
                    "productionAgent:deriveAssetsAgent",
                    "衍生资产",
                    None,
                    &["get_flowData", "add_deriveAsset", "del_deriveAsset"],
                ),
                "run_sub_agent_generate_assets" => (
                    "productionAgent:generateAssetsAgent",
                    "资产生成",
                    None,
                    &["get_flowData", "generate_deriveAsset"],
                ),
                "run_sub_agent_director_plan" => (
                    "productionAgent:directorPlanAgent",
                    "导演规划",
                    Some(("scriptPlan", "scriptPlan")),
                    &["get_flowData", "set_flowData", "use_skill"],
                ),
                "run_sub_agent_storyboard_gen" => (
                    "productionAgent:storyboardGenAgent",
                    "分镜图生成",
                    None,
                    &["get_flowData", "generate_storyboard"],
                ),
                "run_sub_agent_image_edit" => (
                    "productionAgent:storyboardGenAgent",
                    "图片编辑规划",
                    None,
                    &["get_flowData"],
                ),
                "run_sub_agent_storyboard_panel" => (
                    "productionAgent:storyboardPanelAgent",
                    "分镜面板",
                    None,
                    &["get_flowData", "add_flowData_storyboard", "use_skill"],
                ),
                "run_sub_agent_storyboard_table" => (
                    "productionAgent:storyboardTableAgent",
                    "分镜表",
                    Some(("storyboardTable", "storyboardTable")),
                    &["get_flowData"],
                ),
                "run_sub_agent_supervision" => (
                    "productionAgent:supervisionAgent",
                    "监制",
                    None,
                    &["get_flowData"],
                ),
                _ => return Err(AppError::bad_request("不支持的生产子 Agent")),
            };
            let prompt = request
                .arguments
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 prompt"))?;
            if agent_key == "productionAgent:storyboardTableAgent" {
                let script_id = request
                    .script_id
                    .ok_or_else(|| AppError::bad_request("分镜表生成缺少 scriptId"))?;
                let (script, assets) = crate::toonflow_asset_context::load_script_context(
                    &state.pool,
                    request.project_id,
                    script_id,
                )
                .await
                .map_err(|_| AppError::internal("failed to load storyboard planning context"))?;
                if script.trim().is_empty() {
                    return Err(AppError::bad_request(
                        "分镜表生成前置检查失败：当前剧本正文为空",
                    ));
                }
                if assets.as_array().is_none_or(Vec::is_empty) {
                    return Err(AppError::bad_request(
                        "分镜表生成前置检查失败：当前剧本尚未关联可引用资产",
                    ));
                }
                let data: Option<Value> = sqlx::query_scalar("SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id=$2 AND key='productionAgent'")
                    .bind(request.project_id).bind(script_id).fetch_optional(&state.pool).await
                    .map_err(|_| AppError::internal("failed to load storyboard planning workspace"))?;
                let director_plan = data
                    .as_ref()
                    .and_then(|value| value.get("scriptPlan"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if director_plan.trim().is_empty() {
                    return Err(AppError::bad_request(
                        "分镜表生成前置检查失败：请先完成导演规划",
                    ));
                }
            }
            let storyboard_panel_validation = if agent_key == "productionAgent:storyboardPanelAgent"
            {
                let script_id = request
                    .script_id
                    .ok_or_else(|| AppError::bad_request("分镜面板写入缺少 scriptId"))?;
                let mode: String =
                    sqlx::query_scalar("SELECT mode FROM toonflow.projects WHERE id=$1")
                        .bind(request.project_id)
                        .fetch_optional(&state.pool)
                        .await
                        .map_err(|_| AppError::internal("failed to load storyboard panel mode"))?
                        .unwrap_or_else(|| "text".to_string());
                let image_model: String = sqlx::query_scalar(
                    "SELECT lower(coalesce(m.name,'') || ' ' || coalesce(m.key,'') || ' ' || coalesce(m.model,'')) FROM toonflow.projects p LEFT JOIN ai.model_configs m ON m.id=p.image_model WHERE p.id=$1",
                )
                .bind(request.project_id)
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| AppError::internal("failed to load storyboard image model"))?
                .unwrap_or_default();
                let prompt_format =
                    crate::toonflow_storyboard_panel_validation::prompt_format(&image_model);
                let data: Option<Value> = sqlx::query_scalar("SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id=$2 AND key='productionAgent'")
                    .bind(request.project_id).bind(script_id).fetch_optional(&state.pool).await
                    .map_err(|_| AppError::internal("failed to load storyboard table for panel"))?;
                let table = data
                    .as_ref()
                    .and_then(|value| value.get("storyboardTable"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if table.trim().is_empty() {
                    return Err(AppError::bad_request(
                        "分镜面板写入前置检查失败：请先完成分镜表",
                    ));
                }
                let first_frame = mode != "text";
                let expected =
                    crate::toonflow_storyboard_panel_validation::expected_items(table, first_frame);
                if expected.is_empty() {
                    return Err(AppError::bad_request(
                        "分镜面板写入前置检查失败：分镜表中没有可识别的写入单位",
                    ));
                }
                let existing_ids: Vec<i64> = sqlx::query_scalar(
                    "SELECT id FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2",
                )
                .bind(request.project_id)
                .bind(script_id)
                .fetch_all(&state.pool)
                .await
                .map_err(|_| AppError::internal("failed to snapshot storyboard panel"))?;
                Some((first_frame, prompt_format, expected, existing_ids))
            } else {
                None
            };
            if matches!(
                name,
                "run_sub_agent_derive_assets" | "run_sub_agent_generate_assets"
            ) {
                let script_id = request
                    .script_id
                    .ok_or_else(|| AppError::bad_request("人物造型流程缺少 scriptId"))?;
                let missing_extraction =
                    role_names_without_appearances(&state.pool, request.project_id, script_id)
                        .await?;
                if !missing_extraction.is_empty() {
                    return Err(AppError::bad_request(format!(
                        "AI 资产提取尚未保存以下人物的场景服装提示词：{}。请先回到剧本资产阶段重新执行 AI 资产提取。",
                        missing_extraction.join("、")
                    )));
                }
                if name == "run_sub_agent_generate_assets" {
                    let missing =
                        missing_appearance_derivatives(&state.pool, request.project_id, script_id)
                            .await?;
                    if !missing.is_empty() {
                        return Err(AppError::bad_request(format!(
                            "不能生成衍生图片：以下已提取造型尚未创建衍生人物：{}。请先执行人物衍生资产分析。",
                            missing.join("、")
                        )));
                    }
                }
            }
            let system = toonflow_agent_runtime::load_agent_skill(&state.pool, agent_key)
                .await
                .map_err(AppError::bad_request)?;
            let project_info: Option<(String, String, String, String, String)> = sqlx::query_as(
                "SELECT name,type,intro,art_style,video_ratio FROM toonflow.projects WHERE id=$1",
            )
            .bind(request.project_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::bad_request("无法加载项目信息"))?;
            let project_hint = if let Some((name, kind, intro, style, ratio)) = project_info {
                format!(
                    "\n\n## 当前项目\n- 作品名：{name}\n- 小说类型：{kind}\n- 小说简介：{intro}\n- 视觉风格：{style}\n- 视频画幅：{ratio}\n\n**你的所有输出必须与以上项目完全匹配。**"
                )
            } else {
                String::new()
            };
            let generation_gate = if agent_key == "productionAgent:storyboardTableAgent" {
                "\n\n## Toonflow 分镜规划强制执行顺序（不得跳步）\n1. 首轮只调用 get_flowData，依次读取 script、assets、scriptPlan；三项必须全部实际读取，禁止依靠记忆补写。\n2. 先在回复中输出简短的逐场结构化草案：逐条台词按4字/秒估时、划分不超过15秒的片段、写明相邻片段的桥梁元素、标出长台词拆镜点并核对全员视觉落点。\n3. 草案完成后一次性输出且只输出一个完整 <storyboardTable>...</storyboardTable>；标签内部必须是技能模板规定的 Markdown，禁止 JSON、XML 子标签和代码围栏。\n4. 每一行镜头必须能直接生成一张构图明确的静态关键帧：只允许一个时间点、一个机位、一个连续动作状态。禁止蒙太奇、快切、多景别、定格画面、用箭头串联多个动作或在同一行跨时间。\n5. 画面描述只写主体、动作状态和明确空间关系，禁止抽象比喻；运镜只写在运镜列。每个片段镜头时长合计不得超过15秒，超出必须拆段并写清承接。\n6. 画面描述、运镜、音效不得出现光影色调词；画面描述不得重复服装、发型、五官、肤色等资产固有外观。\n7. 每镜含台词最低时长按：台词字数÷4 + 每处标点停顿0.4秒 + 1秒安全余量，最终向上取整；台词必须与剧本逐字一致。\n8. 画面中出现且 assets 已存在的角色、场景、物件，必须全部列入当前片段引用资产名称和ID；未在某行画面出现的人物不得绑定到该行首帧。\n9. 人物在当前场次存在 scenes 匹配的衍生形象时，必须引用该衍生资产的名称和ID，禁止继续引用基础人物。例如医院重伤场景中的王闲必须绑定“重伤绷带”衍生形象。输出前逐镜自检，任一项不满足不得输出。"
            } else {
                ""
            };
            let document_format_gate = if agent_key == "productionAgent:directorPlanAgent" {
                "\n\n## 输出格式强制要求\n<scriptPlan> 标签内部必须直接使用技能模板规定的 Markdown 表格、标题和列表。严禁输出 ```xml 代码围栏，严禁使用 <sceneSummaryTable>、<sceneNotes>、<scene>、<note> 等 XML 子标签，也不得把正文包装成 JSON 对象。"
            } else {
                ""
            };
            let storyboard_panel_mode_gate = if agent_key == "productionAgent:storyboardPanelAgent"
            {
                if storyboard_panel_validation
                    .as_ref()
                    .is_some_and(|(first_frame, _, _, _)| !first_frame)
                {
                    "\n\n## 本次写入路由（服务端已确定）\n必须执行“纯文本多参模式”：以分镜表片段为写入单位，prompt 传 null，shouldGenerateImage 传 false。禁止自行切换模式。"
                } else {
                    match storyboard_panel_validation.as_ref().map(|value| value.1) {
                        Some(
                            crate::toonflow_storyboard_panel_validation::PromptFormat::Seedream,
                        ) => {
                            "\n\n## 本次写入路由（服务端已确定）\n必须执行“首位帧模式”与 Seedream 模式A：分镜表每一行独立写入；prompt 使用中文【画面】【风格】结构，按关联资产顺序声明 @图N，并在【画面】正文用 @图N 替换对应资产名称；shouldGenerateImage 传 true。禁止输出 JSON，禁止自行切换模式。"
                        }
                        Some(
                            crate::toonflow_storyboard_panel_validation::PromptFormat::Nanobanana,
                        ) => {
                            "\n\n## 本次写入路由（服务端已确定）\n必须执行“首位帧模式”与 Nanobanana 模式B：分镜表每一行独立写入；prompt 使用包含 character_reference、continuity_rules、shot、negative 的英文 JSON 结构，每个 @图N 必须在参考声明及 shot 正文中出现；shouldGenerateImage 传 true。禁止自行切换模式。"
                        }
                        _ => {
                            "\n\n## 本次写入路由（服务端已确定）\n必须执行“首位帧模式”：分镜表每一行独立写入，生成忠实的静态首帧 prompt，按关联资产顺序建立 @图N 绑定，shouldGenerateImage 传 true。禁止自行切换模式。"
                        }
                    }
                }
            } else {
                ""
            };
            let role_binding_gate = if matches!(
                agent_key,
                "productionAgent:storyboardTableAgent" | "productionAgent:storyboardPanelAgent"
            ) {
                "\n\n## 人物资产硬性规则\n基础人物只是衍生关系的母资产，一律禁止写入分镜引用。画面中只要出现人物，必须按当前场次的 scenes 匹配并引用对应衍生人物的名称和 ID。如果没有匹配的衍生形象，必须停止写入并先补齐衍生资产，不得回退使用基础人物。"
            } else {
                ""
            };
            let sub_system = format!(
                "{system}\n\n你是 Toonflow 的{label}子 Agent。严格完成委派任务并实际调用要求的工具，不得只用文字声称完成。{generation_gate}{document_format_gate}{storyboard_panel_mode_gate}{role_binding_gate}{project_hint}"
            );
            let mut output = Box::pin(toonflow_agents::run_scoped_production_agent(
                state,
                agent_key,
                &sub_system,
                prompt,
                request.project_id,
                request.script_id,
                allowed_tools,
            ))
            .await?;
            if let Some((first_frame, prompt_format, expected, existing_ids)) =
                &storyboard_panel_validation
            {
                let script_id = request.script_id.expect("panel scriptId checked above");
                let rows: Vec<(i64, String, String, String, i32, String)> = sqlx::query_as(
                    "SELECT id,prompt,coalesce(track,''),coalesce(duration,'0'),should_generate_image,coalesce(video_desc,'') FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2 ORDER BY index,id",
                )
                .bind(request.project_id)
                .bind(script_id)
                .fetch_all(&state.pool)
                .await
                .map_err(|_| AppError::internal("failed to validate storyboard panel rows"))?;
                let new_rows = rows
                    .into_iter()
                    .filter(|row| !existing_ids.contains(&row.0))
                    .collect::<Vec<_>>();
                let mut actual = Vec::with_capacity(new_rows.len());
                for (id, prompt, track, duration, should_generate_image, video_desc) in &new_rows {
                    let asset_ids: Vec<i64> = sqlx::query_scalar(
                        "SELECT asset_id FROM toonflow.assets_storyboards WHERE storyboard_id=$1 ORDER BY sort_order,asset_id",
                    )
                    .bind(id)
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|_| AppError::internal("failed to validate storyboard assets"))?;
                    actual.push(
                        crate::toonflow_storyboard_panel_validation::ActualPanelItem {
                            prompt: prompt.clone(),
                            video_desc: video_desc.clone(),
                            track: track.clone(),
                            duration: duration.parse().unwrap_or_default(),
                            should_generate_image: *should_generate_image != 0,
                            asset_ids,
                        },
                    );
                }
                let expected_asset_ids = expected
                    .iter()
                    .flat_map(|item| item.asset_ids.iter().copied())
                    .collect::<Vec<_>>();
                let role_asset_ids: std::collections::HashSet<i64> = sqlx::query_scalar(
                    "SELECT id FROM toonflow.assets WHERE id=ANY($1) AND type='role'",
                )
                .bind(&expected_asset_ids)
                .fetch_all(&state.pool)
                .await
                .map_err(|_| AppError::internal("failed to load storyboard role assets"))?
                .into_iter()
                .collect();
                let issues = crate::toonflow_storyboard_panel_validation::validate(
                    expected,
                    &actual,
                    *first_frame,
                    *prompt_format,
                    &role_asset_ids,
                );
                if !issues.is_empty() {
                    let new_ids = new_rows.iter().map(|row| row.0).collect::<Vec<_>>();
                    let track_ids: Vec<i64> = sqlx::query_scalar(
                        "SELECT DISTINCT track_id FROM toonflow.storyboards WHERE id=ANY($1) AND track_id IS NOT NULL",
                    )
                    .bind(&new_ids)
                    .fetch_all(&state.pool)
                    .await
                    .unwrap_or_default();
                    let mut tx = state.pool.begin().await.map_err(|_| {
                        AppError::internal("failed to roll back invalid storyboard panel")
                    })?;
                    sqlx::query("DELETE FROM toonflow.storyboards WHERE id=ANY($1)")
                        .bind(&new_ids)
                        .execute(&mut *tx)
                        .await
                        .map_err(|_| AppError::internal("failed to remove invalid panel rows"))?;
                    sqlx::query("DELETE FROM toonflow.video_tracks WHERE id=ANY($1) AND NOT EXISTS (SELECT 1 FROM toonflow.storyboards s WHERE s.track_id=toonflow.video_tracks.id)")
                        .bind(&track_ids)
                        .execute(&mut *tx)
                        .await
                        .map_err(|_| AppError::internal("failed to remove invalid panel tracks"))?;
                    tx.commit().await.map_err(|_| {
                        AppError::internal("failed to commit invalid panel rollback")
                    })?;
                    return Err(AppError::bad_request(format!(
                        "分镜面板写入未通过 Toonflow 对账，已撤销本次错误写入：{}",
                        issues.join("；")
                    )));
                }
                let track_ids: Vec<i64> = sqlx::query_scalar(
                    "SELECT DISTINCT track_id FROM toonflow.storyboards WHERE id=ANY($1) AND track_id IS NOT NULL ORDER BY track_id",
                )
                .bind(new_rows.iter().map(|row| row.0).collect::<Vec<_>>())
                .fetch_all(&state.pool)
                .await
                .map_err(|_| AppError::internal("failed to load new video tracks"))?;
                let video_setting: Option<(Option<i64>, String)> =
                    sqlx::query_as("SELECT video_model,mode FROM toonflow.projects WHERE id=$1")
                        .bind(request.project_id)
                        .fetch_optional(&state.pool)
                        .await
                        .map_err(|_| AppError::internal("failed to load video prompt settings"))?;
                if let Some((Some(model), mode)) = video_setting.filter(|_| !track_ids.is_empty()) {
                    sqlx::query(
                        "UPDATE toonflow.video_tracks SET state='生成中',reason=NULL WHERE id=ANY($1)",
                    )
                    .bind(&track_ids)
                    .execute(&state.pool)
                    .await
                    .map_err(|_| AppError::internal("failed to start video prompts"))?;
                    let pool = state.pool.clone();
                    let project_id = request.project_id;
                    tokio::spawn(async move {
                        for track_id in track_ids {
                            let _ = crate::toonflow_video::create_prompt(
                                &pool,
                                track_id,
                                project_id,
                                &model.to_string(),
                                &mode,
                            )
                            .await;
                        }
                    });
                    output.push_str(
                        "\n\n视频提示词已开始按轨道自动生成，完成后会直接回填到每个轨道。",
                    );
                }
            }
            if agent_key == "productionAgent:deriveAssetsAgent" {
                let script_id = request
                    .script_id
                    .ok_or_else(|| AppError::bad_request("衍生资产分析缺少 scriptId"))?;
                let missing =
                    missing_appearance_derivatives(&state.pool, request.project_id, script_id)
                        .await?;
                if !missing.is_empty() {
                    let repair_prompt = format!(
                        "上轮未完整引用资产提取阶段的造型。以下 appearance 尚无衍生人物：{}。立即重新读取 assets，逐项原样复制 appearance.costumePrompt，并携带对应 appearanceId 调用 add_deriveAsset；禁止临时改写服装。",
                        missing.join("、")
                    );
                    output = Box::pin(toonflow_agents::run_scoped_production_agent(
                        state,
                        agent_key,
                        &sub_system,
                        &repair_prompt,
                        request.project_id,
                        request.script_id,
                        allowed_tools,
                    ))
                    .await?;
                    let still_missing =
                        missing_appearance_derivatives(&state.pool, request.project_id, script_id)
                            .await?;
                    if !still_missing.is_empty() {
                        return Err(AppError::bad_request(format!(
                            "衍生资产分析未覆盖全部出场人物，仍缺少：{}",
                            still_missing.join("、")
                        )));
                    }
                }
            }
            if agent_key == "productionAgent:storyboardTableAgent" {
                let script_id = request
                    .script_id
                    .ok_or_else(|| AppError::bad_request("分镜表生成缺少 scriptId"))?;
                let (_, assets) = crate::toonflow_asset_context::load_script_context(
                    &state.pool,
                    request.project_id,
                    script_id,
                )
                .await
                .map_err(|_| AppError::internal("failed to load storyboard validation assets"))?;
                let names = crate::toonflow_storyboard_table_validation::asset_names(&assets);
                for repair_round in 0..=2 {
                    let content = tagged(&output, "storyboardTable").ok_or_else(|| {
                        AppError::bad_request("分镜表 Agent 未输出完整 storyboardTable 标签")
                    })?;
                    let issues =
                        crate::toonflow_storyboard_table_validation::validate(&content, &names);
                    if issues.is_empty() {
                        break;
                    }
                    if repair_round == 2 {
                        return Err(AppError::bad_request(format!(
                            "分镜表连续修复后仍未通过写入门禁：{}",
                            issues.join("；")
                        )));
                    }
                    let repair_prompt = format!(
                        "上一版分镜表未通过写入门禁，禁止询问用户。严格重新执行 Toonflow 分镜规划流程：重新读取 script、assets、scriptPlan，先给出修复草案（估时、拆片段、桥梁元素、拆镜点、全员视觉落点），再一次性输出一份完整修正版 <storyboardTable>。必须逐项修复：\n- {}\n台词原文不得改写；机械问题全部修复后才能输出。",
                        issues.join("\n- ")
                    );
                    output = Box::pin(toonflow_agents::run_scoped_production_agent(
                        state,
                        agent_key,
                        &sub_system,
                        &repair_prompt,
                        request.project_id,
                        request.script_id,
                        allowed_tools,
                    ))
                    .await?;
                }
            }
            if let (Some((tag, key)), Some(script_id)) = (flow_tag, request.script_id) {
                if let Some(content) = tagged(&output, tag) {
                    let mut data:Value=sqlx::query_scalar("SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id=$2 AND key='productionAgent'").bind(request.project_id).bind(script_id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to load production workspace"))?.unwrap_or_else(||json!({}));
                    data[key] = json!(content);
                    sqlx::query("INSERT INTO toonflow.agent_work_data(project_id,episodes_id,key,data,create_time,update_time)VALUES($1,$2,'productionAgent',$3,$4,$4) ON CONFLICT(project_id,episodes_id,key) DO UPDATE SET data=excluded.data,update_time=excluded.update_time").bind(request.project_id).bind(script_id).bind(data).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to save production sub agent result"))?;
                }
            }
            if agent_key == "productionAgent:storyboardTableAgent" {
                let supervision_key = "productionAgent:supervisionAgent";
                let supervision_skill =
                    toonflow_agent_runtime::load_agent_skill(&state.pool, supervision_key)
                        .await
                        .map_err(AppError::bad_request)?;
                let supervision_system = format!(
                    "{supervision_skill}\n\n你是 Toonflow 的监制。分镜表刚刚完成生成或修复并已写入工作区。必须重新读取当前 storyboardTable、script、assets，给出完整审核报告和新的 A/B/C/D 评分；不得沿用上一次评分。{project_hint}"
                );
                let audit = Box::pin(toonflow_agents::run_scoped_production_agent(
                    state,
                    supervision_key,
                    &supervision_system,
                    "请立即复审当前最新分镜表，输出完整问题清单和新的评分。",
                    request.project_id,
                    request.script_id,
                    &["get_flowData"],
                ))
                .await?;
                let reviewed_content = format!(
                    "{output}\n\n---\n\n## 自动复审结果（基于修复后的最新分镜表）\n{audit}"
                );
                return Ok(json!({
                    "agent": agent_key,
                    "content": reviewed_content,
                    "automaticReview": true,
                    "review": audit
                }));
            }
            Ok(json!({"agent":agent_key,"content":output}))
        }
        ("productionAgent", "set_flowData") => {
            let script_id = request
                .script_id
                .ok_or_else(|| AppError::bad_request("生产工具缺少 scriptId"))?;
            let key = request
                .arguments
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 key"))?;
            if !matches!(
                key,
                "scriptPlan" | "storyboardTable" | "script" | "assets" | "storyboard"
            ) {
                return Err(AppError::bad_request("不支持的 Flow key"));
            }
            let value = request
                .arguments
                .get("value")
                .cloned()
                .unwrap_or(Value::Null);
            let mut data:Value=sqlx::query_scalar("SELECT data FROM toonflow.agent_work_data WHERE project_id=$1 AND episodes_id=$2 AND key='productionAgent'").bind(request.project_id).bind(script_id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to load flow data"))?.unwrap_or_else(||json!({}));
            data[key] = value;
            sqlx::query("INSERT INTO toonflow.agent_work_data(project_id,episodes_id,key,data,create_time,update_time)VALUES($1,$2,'productionAgent',$3,$4,$4) ON CONFLICT(project_id,episodes_id,key) DO UPDATE SET data=excluded.data,update_time=excluded.update_time").bind(request.project_id).bind(script_id).bind(&data).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to save flow data"))?;
            Ok(json!({"key":key,"data":data[key]}))
        }
        ("productionAgent", "add_flowData_storyboard") => {
            let script_id = request
                .script_id
                .ok_or_else(|| AppError::bad_request("生产工具缺少 scriptId"))?;
            let id = now_ms() * 1000;
            let duration = request
                .arguments
                .get("duration")
                .and_then(Value::as_i64)
                .unwrap_or(4)
                .clamp(1, 60);
            let should = request
                .arguments
                .get("shouldGenerateImage")
                .map(|value| {
                    value
                        .as_bool()
                        .unwrap_or_else(|| value.as_str() != Some("false"))
                })
                .unwrap_or(true);
            let associated_asset_ids = request
                .arguments
                .get("associateAssetsIds")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                .unwrap_or_default();
            crate::toonflow_storyboard_asset_validation::reject_base_role_asset_ids(
                &state.pool,
                &associated_asset_ids,
            )
            .await?;
            let index:i32=sqlx::query_scalar("SELECT coalesce(max(index),-1)+1 FROM toonflow.storyboards WHERE project_id=$1 AND script_id=$2").bind(request.project_id).bind(script_id).fetch_one(&state.pool).await.unwrap_or(0);
            let track_id = id + 1;
            let mut tx = state
                .pool
                .begin()
                .await
                .map_err(|_| AppError::internal("failed to add storyboard"))?;
            sqlx::query("INSERT INTO toonflow.video_tracks(id,project_id,script_id,state,duration)VALUES($1,$2,$3,'未生成',$4)").bind(track_id).bind(request.project_id).bind(script_id).bind(duration as i32).execute(&mut *tx).await.map_err(|_|AppError::internal("failed to add storyboard track"))?;
            sqlx::query("INSERT INTO toonflow.storyboards(id,script_id,prompt,duration,state,track_id,track,video_desc,should_generate_image,project_id,index,create_time)VALUES($1,$2,$3,$4,'未生成',$5,$6,$7,$8,$9,$10,$11)").bind(id).bind(script_id).bind(request.arguments.get("prompt").and_then(Value::as_str).unwrap_or_default()).bind(duration.to_string()).bind(track_id).bind(request.arguments.get("track").and_then(Value::as_str).unwrap_or("main")).bind(request.arguments.get("videoDesc").and_then(Value::as_str).unwrap_or_default()).bind(if should{1}else{0}).bind(request.project_id).bind(index).bind(now_ms()).execute(&mut *tx).await.map_err(|_|AppError::internal("failed to add storyboard"))?;
            if !associated_asset_ids.is_empty() {
                for (sort_order, asset_id) in associated_asset_ids.into_iter().enumerate() {
                    sqlx::query("INSERT INTO toonflow.assets_storyboards(storyboard_id,asset_id,sort_order)VALUES($1,$2,$3) ON CONFLICT(storyboard_id,asset_id) DO UPDATE SET sort_order=excluded.sort_order").bind(id).bind(asset_id).bind(sort_order as i32).execute(&mut *tx).await.map_err(|_|AppError::internal("failed to bind storyboard asset"))?;
                }
            }
            tx.commit()
                .await
                .map_err(|_| AppError::internal("failed to add storyboard"))?;
            Ok(json!({"id":id,"index":index}))
        }
        ("productionAgent", "update_storyboard") => {
            let id = request
                .arguments
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::bad_request("缺少 id"))?;
            let result=sqlx::query("UPDATE toonflow.storyboards SET prompt=coalesce($3,prompt),video_desc=coalesce($4,video_desc),duration=coalesce($5,duration),track=coalesce($6,track),should_generate_image=coalesce($7,should_generate_image) WHERE id=$1 AND project_id=$2").bind(id).bind(request.project_id).bind(request.arguments.get("prompt").and_then(Value::as_str)).bind(request.arguments.get("videoDesc").and_then(Value::as_str)).bind(request.arguments.get("duration").and_then(Value::as_i64).map(|v|v.to_string())).bind(request.arguments.get("track").and_then(Value::as_str)).bind(request.arguments.get("shouldGenerateImage").and_then(Value::as_bool).map(|v|if v{1}else{0})).execute(&state.pool).await.map_err(|_|AppError::internal("failed to update storyboard"))?;
            if result.rows_affected() == 0 {
                return Err(AppError::not_found("storyboard not found"));
            }
            Ok(json!(true))
        }
        ("productionAgent", "generate_storyboard") => {
            let script_id = request
                .script_id
                .ok_or_else(|| AppError::bad_request("生产工具缺少 scriptId"))?;
            let ids = request
                .arguments
                .get("ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_i64())
                .collect::<Vec<_>>();
            let rows = toonflow_image_workflow::schedule_storyboard_generation(
                &state.pool,
                request.project_id,
                script_id,
                &ids,
                request
                    .arguments
                    .get("concurrentCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(5) as usize,
                false,
            )
            .await?;
            Ok(json!(rows))
        }
        ("productionAgent", "delete_storyboard") => {
            let ids = request
                .arguments
                .get("ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_i64())
                .collect::<Vec<_>>();
            sqlx::query("DELETE FROM toonflow.storyboards WHERE id=ANY($1) AND project_id=$2")
                .bind(&ids)
                .bind(request.project_id)
                .execute(&state.pool)
                .await
                .map_err(|_| AppError::internal("failed to delete storyboards"))?;
            Ok(json!({"ids":ids}))
        }
        _ => Err(AppError::bad_request("不支持的 Agent 工具")),
    }
}
