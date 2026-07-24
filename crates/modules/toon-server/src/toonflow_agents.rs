use crate::{
    ToonState, ai_client, shared::require, toonflow_agent_runtime, toonflow_agent_tools,
    toonflow_ws::WsEmitter,
};
use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;
use tracing::warn;

static ACTIVE_RUNS: LazyLock<Mutex<HashMap<i64, tokio::task::AbortHandle>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn next_id(offset: i64) -> i64 {
    now_ms() * 1000 + offset
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub agent_type: String,
    pub isolation_key: String,
    pub project_id: i64,
    pub script_id: Option<i64>,
    pub content: String,
    #[serde(default)]
    pub think: bool,
    #[serde(default)]
    pub think_level: i32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    agent_type: String,
    isolation_key: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearMemoryRequest {
    agent_type: String,
    isolation_key: String,
    memory_type: Option<String>,
}
#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRow {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub memory_type: String,
    pub create_time: i64,
}
#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RunRow {
    id: i64,
    agent_type: String,
    isolation_key: String,
    project_id: i64,
    script_id: Option<i64>,
    input: String,
    output: Option<String>,
    state: String,
    error_reason: Option<String>,
    think: bool,
    think_level: i32,
    start_time: i64,
    finish_time: Option<i64>,
}

#[derive(Deserialize)]
pub struct RunIdRequest {
    id: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRequest {
    run_id: i64,
    #[serde(default)]
    after_id: i64,
}
#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    id: i64,
    run_id: i64,
    event_type: String,
    data: Value,
    create_time: i64,
}

fn validate_agent(value: &str) -> Result<&'static str, AppError> {
    match value {
        "scriptAgent" => Ok("scriptAgent:decisionAgent"),
        "productionAgent" => Ok("productionAgent:decisionAgent"),
        _ => Err(AppError::bad_request(
            "agentType 仅支持 scriptAgent 或 productionAgent",
        )),
    }
}

/// Public version used by the WebSocket handler.
pub fn agent_key_for(agent_type: &str) -> Result<&'static str, String> {
    match agent_type {
        "scriptAgent" => Ok("scriptAgent:decisionAgent"),
        "productionAgent" => Ok("productionAgent:decisionAgent"),
        _ => Err("agentType 仅支持 scriptAgent 或 productionAgent".into()),
    }
}

async fn project_context(state: &ToonState, request: &ChatRequest) -> Result<String, AppError> {
    let project: Option<(String,String,String,String,String,Option<i64>,Option<i64>,String)> = sqlx::query_as("SELECT name,type,intro,art_style,video_ratio,image_model,video_model,mode FROM toonflow.projects WHERE id=$1")
        .bind(request.project_id).fetch_optional(&state.pool).await.map_err(|_| AppError::internal("failed to load agent project"))?;
    let (name, kind, intro, style, ratio, image_model, video_model, mode) =
        project.ok_or_else(|| AppError::not_found("project not found"))?;
    if request.agent_type == "scriptAgent" {
        let chapters: i64 =
            sqlx::query_scalar("SELECT count(*) FROM toonflow.novels WHERE project_id=$1")
                .bind(request.project_id)
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0);
        Ok(format!(
            "## 项目信息\n小说名称：{name}\n小说类型：{kind}\n小说简介：{intro}\n视觉风格：{style}\n视频画幅：{ratio}\n章节数量：{chapters}章\n\n**重要**：平台规格=视频画幅({ratio})，风格定位=小说类型({kind})+视觉风格({style})。这两项参数已由项目配置确定，无需再向用户确认，直接使用即可。"
        ))
    } else {
        let script = if let Some(id) = request.script_id {
            sqlx::query_as::<_, (String, String)>(
                "SELECT name,content FROM toonflow.scripts WHERE id=$1 AND project_id=$2",
            )
            .bind(id)
            .bind(request.project_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| AppError::internal("failed to load agent script"))?
        } else {
            None
        };
        let script_context = script
            .map(|(n, c)| format!("\n当前剧本：{n}\n剧本内容：{c}"))
            .unwrap_or_default();
        Ok(format!(
            "## 生产上下文\n项目：{name}\n图像模型 ID：{}\n视频模型 ID：{}\n视频画幅：{ratio}\n视频生成模式：{mode}\n分镜面板写入模式：{}{script_context}",
            image_model
                .map(|x| x.to_string())
                .unwrap_or_else(|| "未配置".into()),
            video_model
                .map(|x| x.to_string())
                .unwrap_or_else(|| "未配置".into()),
            if mode == "text" {
                "纯文本多参模式"
            } else {
                "首位帧模式"
            }
        ))
    }
}

async fn memory_context(state: &ToonState, request: &ChatRequest) -> Result<String, AppError> {
    let summaries:Vec<String>=sqlx::query_scalar("SELECT content FROM toonflow.agent_memories WHERE agent_type=$1 AND isolation_key=$2 AND memory_type='summary' ORDER BY create_time DESC LIMIT 5").bind(&request.agent_type).bind(&request.isolation_key).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to load agent memory"))?;
    let messages:Vec<(String,String)>=sqlx::query_as("SELECT role,content FROM toonflow.agent_memories WHERE agent_type=$1 AND isolation_key=$2 AND memory_type='message' ORDER BY create_time DESC LIMIT 8").bind(&request.agent_type).bind(&request.isolation_key).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to load agent memory"))?;
    let recent = messages
        .into_iter()
        .rev()
        .map(|(r, c)| format!("{r}: {c}"))
        .collect::<Vec<_>>()
        .join("\n");
    let relevant = toonflow_agent_runtime::relevant_memories(
        &state.pool,
        &request.agent_type,
        &request.isolation_key,
        &request.content,
        5,
    )
    .await
    .map_err(|_| AppError::internal("failed to retrieve relevant agent memory"))?;
    Ok(format!(
        "## Memory\n以下是内部记忆，不要主动说明记忆机制。\n相关记忆：\n{}\n历史摘要：\n{}\n近期对话：\n{recent}",
        relevant.join("\n"),
        summaries.into_iter().rev().collect::<Vec<_>>().join("\n"),
    ))
}

async fn add_memory(
    state: &ToonState,
    agent: &str,
    isolation: &str,
    role: &str,
    content: &str,
) -> Result<i64, AppError> {
    let id = next_id(if role == "user" { 1 } else { 2 });
    sqlx::query("INSERT INTO toonflow.agent_memories(id,agent_type,isolation_key,role,content,create_time) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(agent).bind(isolation).bind(role).bind(content).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to save agent memory"))?;
    toonflow_agent_runtime::store_memory_embedding(&state.pool, id, content).await;
    Ok(id)
}

async fn summarize_if_needed(state: &ToonState, request: &ChatRequest, agent_key: &str) {
    let rows:Vec<(i64,String,String)>=sqlx::query_as("SELECT id,role,content FROM toonflow.agent_memories WHERE agent_type=$1 AND isolation_key=$2 AND memory_type='message' AND summarized=false ORDER BY create_time LIMIT 6").bind(&request.agent_type).bind(&request.isolation_key).fetch_all(&state.pool).await.unwrap_or_default();
    if rows.len() < 6 {
        return;
    }
    let source = rows
        .iter()
        .map(|(_, r, c)| format!("{r}: {c}"))
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(summary) = ai_client::text(
        &state.pool,
        agent_key,
        "将对话压缩为500字以内的事实摘要，只输出摘要。",
        &source,
    )
    .await
    else {
        return;
    };
    let ids = rows.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
    let Ok(mut tx) = state.pool.begin().await else {
        return;
    };
    if sqlx::query("INSERT INTO toonflow.agent_memories(id,agent_type,isolation_key,role,content,memory_type,related_message_ids,create_time) VALUES($1,$2,$3,'system',$4,'summary',$5,$6)").bind(next_id(3)).bind(&request.agent_type).bind(&request.isolation_key).bind(summary).bind(json!(ids)).bind(now_ms()).execute(&mut *tx).await.is_err(){return}
    let _ = sqlx::query("UPDATE toonflow.agent_memories SET summarized=true WHERE id=ANY($1)")
        .bind(&ids)
        .execute(&mut *tx)
        .await;
    let _ = tx.commit().await;
}

fn tool_guide(agent_type: &str) -> &'static str {
    if agent_type == "scriptAgent" {
        r#"可用工具：get_novel_events({chapterIndexs}), get_novel_text({chapterIndex}), get_script_content({ids}), run_sub_agent_storySkeleton({prompt}), run_sub_agent_adaptationStrategy({prompt}), run_sub_agent_script({prompt}), run_supervision_agent({prompt}), deepRetrieve({keyword})。
自动生成剧本任务还可调用 save_scripts({scripts:[{name,content}]}）。剧本子 Agent 返回后必须调用它写入项目，未保存不得宣称完成。
deepRetrieve 用于搜索历史对话中的关键信息，仅在用户要求回想时使用。
需要调用工具时，仅输出一个或多个如下标签，不要编造结果：
<tool_call>{"name":"工具名","arguments":{}}</tool_call>"#
    } else {
        r#"可用工具：get_flowData({key}), set_flowData({key,value}), add_deriveAsset({assetsId,id,name,desc}), del_deriveAsset({id}), generate_deriveAsset({ids,concurrentCount}), add_flowData_storyboard({videoDesc,prompt,track,duration,associateAssetsIds,shouldGenerateImage}), update_storyboard({id,...}), generate_storyboard({ids,concurrentCount}), delete_storyboard({ids}), get_video_workbench({}), generate_video_prompt({trackId}), update_video_prompt({trackId,prompt}), select_video({trackId,videoId}), run_sub_agent_derive_assets({prompt}), run_sub_agent_generate_assets({prompt}), run_sub_agent_director_plan({prompt}), run_sub_agent_storyboard_gen({prompt}), run_sub_agent_storyboard_panel({prompt}), run_sub_agent_storyboard_table({prompt}), run_sub_agent_supervision({prompt})。
需要调用工具时，仅输出一个或多个如下标签，不要编造结果：
<tool_call>{"name":"工具名","arguments":{}}</tool_call>"#
    }
}

fn thinking_instruction(enabled: bool, level: i32) -> &'static str {
    if !enabled {
        return "直接处理任务，不展示内部推理过程。";
    }
    match level.clamp(0, 3) {
        0 => "进行简短检查后作答，不展示内部推理过程。",
        1 => "进行基础分析和约束检查，不展示内部推理过程。",
        2 => "进行较深入的多步分析、工具规划和结果复核，不展示内部推理过程。",
        _ => "进行完整的多方案分析、工具规划、交叉检查和最终复核，不展示内部推理过程。",
    }
}

fn parse_tool_calls(text: &str) -> Vec<(String, Value)> {
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool_call>") {
        let content = &rest[start + 11..];
        let Some(end) = content.find("</tool_call>") else {
            break;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&content[..end]) {
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                calls.push((
                    name.to_string(),
                    value.get("arguments").cloned().unwrap_or_else(|| json!({})),
                ));
            }
        }
        rest = &content[end + 12..];
    }
    calls
}

fn tool_names(agent_type: &str) -> &'static [&'static str] {
    if agent_type == "scriptAgent" {
        &[
            "get_novel_events",
            "get_novel_text",
            "get_script_content",
            "run_sub_agent_storySkeleton",
            "run_sub_agent_adaptationStrategy",
            "run_sub_agent_script",
            "run_supervision_agent",
            "save_scripts",
            "use_skill",
            "deepRetrieve",
        ]
    } else {
        &[
            "get_flowData",
            "set_flowData",
            "add_deriveAsset",
            "del_deriveAsset",
            "generate_deriveAsset",
            "add_flowData_storyboard",
            "update_storyboard",
            "generate_storyboard",
            "delete_storyboard",
            "get_video_workbench",
            "generate_video_prompt",
            "update_video_prompt",
            "select_video",
            "run_sub_agent_derive_assets",
            "run_sub_agent_generate_assets",
            "run_sub_agent_director_plan",
            "run_sub_agent_storyboard_gen",
            "run_sub_agent_storyboard_panel",
            "run_sub_agent_storyboard_table",
            "run_sub_agent_supervision",
            "use_skill",
        ]
    }
}

fn tool_def(name: &str) -> Value {
    match name {
        "get_novel_events" => {
            json!({"type":"function","function":{"name":"get_novel_events","description":"获取项目中的章节事件列表。返回章节号、标题和已提取的事件数据。校验章节范围或读取事件时使用。","parameters":{"type":"object","properties":{"chapterIndexs":{"type":"array","items":{"type":"integer"},"description":"需要查询的章节编号数组"}},"required":["chapterIndexs"]}}})
        }
        "get_novel_text" => {
            json!({"type":"function","function":{"name":"get_novel_text","description":"获取指定章节的原始小说文本内容。需要读取原文细节时使用。","parameters":{"type":"object","properties":{"chapterIndex":{"type":"integer","description":"章节编号"}},"required":["chapterIndex"]}}})
        }
        "get_script_content" => {
            json!({"type":"function","function":{"name":"get_script_content","description":"读取已有剧本内容。用于执行层子Agent读取已生成的剧本。","parameters":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"integer"},"description":"剧本ID数组"}},"required":["ids"]}}})
        }
        "run_sub_agent_storySkeleton" => {
            json!({"type":"function","function":{"name":"run_sub_agent_storySkeleton","description":"派发任务给「编剧」执行层子Agent，进行故事骨架搭建（阶段1）。项目参数确认且章节校验通过后调用。仅需传入prompt参数。完成后必须调用run_supervision_agent审核。","parameters":{"type":"object","properties":{"prompt":{"type":"string","description":"派发给子Agent的执行指令（正文≤100字），需在头部附带【项目配置】"}},"required":["prompt"]}}})
        }
        "run_sub_agent_adaptationStrategy" => {
            json!({"type":"function","function":{"name":"run_sub_agent_adaptationStrategy","description":"派发任务给「编剧」执行层子Agent，进行改编策略制定（阶段2）。阶段1审核通过后调用。仅需传入prompt参数。完成后必须调用run_supervision_agent审核。","parameters":{"type":"object","properties":{"prompt":{"type":"string","description":"派发给子Agent的执行指令（正文≤100字），需在头部附带【项目配置】"}},"required":["prompt"]}}})
        }
        "run_sub_agent_script" => {
            json!({"type":"function","function":{"name":"run_sub_agent_script","description":"【阶段3】派发剧本编写任务给「编剧」执行层子Agent。每次调用只生成一集剧本。必须按集序逐集调用，全部集数完成后统一调用save_scripts写入。仅需传入prompt参数。","parameters":{"type":"object","properties":{"prompt":{"type":"string","description":"派发给子Agent的执行指令（正文≤100字），需指定当前第几集和总集数（如：请编写第1/3集剧本）"}},"required":["prompt"]}}})
        }
        "run_supervision_agent" => {
            json!({"type":"function","function":{"name":"run_supervision_agent","description":"派发审核任务给「编辑」监督层子Agent。阶段1和阶段2的子Agent正常完成后必须立即调用，不可跳过。审核返回A/B/C/D评分，决策层必须展示报告并等待用户回复后才继续。","parameters":{"type":"object","properties":{"prompt":{"type":"string","description":"审核指令，指定审核对象和维度"}},"required":["prompt"]}}})
        }
        "save_scripts" => {
            json!({"type":"function","function":{"name":"save_scripts","description":"将生成的剧本批量写入项目数据库。子Agent完成剧本编写后必须调用此工具保存，未保存不得宣称完成。","parameters":{"type":"object","properties":{"scripts":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string","description":"剧本名称"},"content":{"type":"string","description":"剧本完整内容"}},"required":["name","content"]},"description":"剧本数组"}},"required":["scripts"]}}})
        }
        "deepRetrieve" => {
            json!({"type":"function","function":{"name":"deepRetrieve","description":"搜索历史对话和记忆中的相关信息。仅在用户明确要求回想、回顾、查看之前的内容时才调用。决策层不主动调用此工具。","parameters":{"type":"object","properties":{"keyword":{"type":"string","description":"搜索关键词"}},"required":["keyword"]}}})
        }
        "use_skill" => {
            json!({"type":"function","function":{"name":"use_skill","description":"加载动态Skill内容。需要专项技法参考时使用，传入skill文件路径即可获取完整内容。","parameters":{"type":"object","properties":{"path":{"type":"string","description":"Skill文件路径，如 production_skills/storyboard_prompt_techniques"}},"required":["path"]}}})
        }
        // Production agent tools
        "get_flowData" => {
            json!({"type":"function","function":{"name":"get_flowData","description":"读取生产工作区数据。可读取剧本、导演规划、资产列表、分镜表、分镜面板等。","parameters":{"type":"object","properties":{"key":{"type":"string","description":"数据key: script/scriptPlan/assets/storyboardTable/storyboard"}},"required":["key"]}}})
        }
        "set_flowData" => {
            json!({"type":"function","function":{"name":"set_flowData","description":"写入生产工作区数据。保存导演规划、分镜表等产出物。","parameters":{"type":"object","properties":{"key":{"type":"string","description":"数据key"},"value":{"type":"object","description":"要写入的数据"}},"required":["key","value"]}}})
        }
        "add_deriveAsset" => {
            json!({"type":"function","function":{"name":"add_deriveAsset","description":"根据资产提取阶段保存的人物造型新增或更新衍生人物资产。","parameters":{"type":"object","properties":{"assetsId":{"type":"integer"},"appearanceId":{"type":"integer","description":"get_flowData 返回的 appearance.id"},"id":{"type":"integer"},"name":{"type":"string"},"desc":{"type":"string"}},"required":["assetsId","appearanceId","name","desc"]}}})
        }
        "del_deriveAsset" => {
            json!({"type":"function","function":{"name":"del_deriveAsset","description":"删除衍生资产。","parameters":{"type":"object","properties":{"assetsId":{"type":"integer"},"id":{"type":"integer"}},"required":["assetsId","id"]}}})
        }
        "generate_deriveAsset" => {
            json!({"type":"function","function":{"name":"generate_deriveAsset","description":"触发衍生资产的图片生成（异步任务）。传入资产ID数组，可设置并发数。","parameters":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"integer"}},"concurrentCount":{"type":"integer"}},"required":["ids"]}}})
        }
        "add_flowData_storyboard" => {
            json!({"type":"function","function":{"name":"add_flowData_storyboard","description":"向分镜面板新增一条分镜记录。包含画面描述、提示词、轨道、时长、关联资产。","parameters":{"type":"object","properties":{"videoDesc":{"type":"string"},"prompt":{"type":"string"},"track":{"type":"string"},"duration":{"type":"integer"},"associateAssetsIds":{"type":"array","items":{"type":"integer"}},"shouldGenerateImage":{"type":"string"}},"required":["videoDesc","track","duration"]}}})
        }
        "update_storyboard" => {
            json!({"type":"function","function":{"name":"update_storyboard","description":"更新已有分镜记录的字段。只更新传入的字段。","parameters":{"type":"object","properties":{"id":{"type":"integer"},"videoDesc":{"type":"string"},"prompt":{"type":"string"},"track":{"type":"string"},"duration":{"type":"integer"}},"required":["id"]}}})
        }
        "generate_storyboard" => {
            json!({"type":"function","function":{"name":"generate_storyboard","description":"触发分镜图片生成（异步任务）。传入分镜ID数组。","parameters":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"integer"}},"concurrentCount":{"type":"integer"}},"required":["ids"]}}})
        }
        "delete_storyboard" => {
            json!({"type":"function","function":{"name":"delete_storyboard","description":"批量删除分镜记录。","parameters":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"integer"}}},"required":["ids"]}}})
        }
        "get_video_workbench" => {
            json!({"type":"function","function":{"name":"get_video_workbench","description":"读取当前剧本的视频工作台、视频轨道、提示词、生成状态和候选视频。","parameters":{"type":"object","properties":{}}}})
        }
        "generate_video_prompt" => {
            json!({"type":"function","function":{"name":"generate_video_prompt","description":"为指定视频轨道生成视频提示词。","parameters":{"type":"object","properties":{"trackId":{"type":"integer"}},"required":["trackId"]}}})
        }
        "update_video_prompt" => {
            json!({"type":"function","function":{"name":"update_video_prompt","description":"更新指定视频轨道的视频提示词。","parameters":{"type":"object","properties":{"trackId":{"type":"integer"},"prompt":{"type":"string"}},"required":["trackId","prompt"]}}})
        }
        "select_video" => {
            json!({"type":"function","function":{"name":"select_video","description":"从指定轨道的成功候选视频中确认选用一个视频。","parameters":{"type":"object","properties":{"trackId":{"type":"integer"},"videoId":{"type":"integer"}},"required":["trackId","videoId"]}}})
        }
        "run_sub_agent_derive_assets" => {
            json!({"type":"function","function":{"name":"run_sub_agent_derive_assets","description":"【阶段2】派发衍生资产分析任务给执行层「执行导演」子Agent。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        "run_sub_agent_generate_assets" => {
            json!({"type":"function","function":{"name":"run_sub_agent_generate_assets","description":"【阶段3】派发衍生资产生成任务给执行层子Agent。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        "run_sub_agent_director_plan" => {
            json!({"type":"function","function":{"name":"run_sub_agent_director_plan","description":"【阶段1】派发导演规划任务给执行层「执行导演」子Agent。分析剧本、拆分场次、输出拍摄计划。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        "run_sub_agent_storyboard_gen" => {
            json!({"type":"function","function":{"name":"run_sub_agent_storyboard_gen","description":"【阶段6】派发分镜图生成任务给执行层子Agent。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        "run_sub_agent_storyboard_panel" => {
            json!({"type":"function","function":{"name":"run_sub_agent_storyboard_panel","description":"【阶段5】派发分镜面板写入任务给执行层子Agent。将分镜表逐条写入面板。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        "run_sub_agent_storyboard_table" => {
            json!({"type":"function","function":{"name":"run_sub_agent_storyboard_table","description":"【阶段4】派发分镜表构建或修复任务给执行层子Agent。成功写入后会自动复审并在同一结果中返回基于最新版本的 A/B/C/D 评分；同一轮无需再次调用监制。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        "run_sub_agent_supervision" => {
            json!({"type":"function","function":{"name":"run_sub_agent_supervision","description":"【审核】派发审核任务给监督层「监制」子Agent。对分镜表进行质量审核。","parameters":{"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"]}}})
        }
        _ => {
            json!({"type":"function","function":{"name":name,"description":format!("Toonflow Agent 工具: {name}"),"parameters":{"type":"object","properties":{},"additionalProperties":true}}})
        }
    }
}

fn native_tool_definitions(agent_type: &str) -> Vec<Value> {
    tool_names(agent_type)
        .iter()
        .map(|name| tool_def(name))
        .collect()
}

pub(crate) async fn run_scoped_production_agent(
    state: &ToonState,
    agent_key: &str,
    system: &str,
    prompt: &str,
    project_id: i64,
    script_id: Option<i64>,
    allowed_tools: &[&str],
) -> Result<String, AppError> {
    let definitions = allowed_tools
        .iter()
        .map(|name| tool_def(name))
        .collect::<Vec<_>>();
    let mut messages = vec![
        json!({"role":"system","content":format!("{system}\n\n{}", tool_guide("productionAgent"))}),
        json!({"role":"user","content":prompt}),
    ];
    for _ in 0..24 {
        let raw = ai_client::text_tools(
            &state.pool,
            agent_key,
            messages.clone(),
            definitions.clone(),
        )
        .await
        .map_err(AppError::bad_request)?;
        let message = raw
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| AppError::bad_request("执行层 Agent 响应缺少 message"))?;
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            return Ok(message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string());
        }
        messages.push(message);
        for call in calls {
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("执行层工具调用缺少 id"))?;
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("执行层工具调用缺少名称"))?;
            if !allowed_tools.contains(&name) {
                return Err(AppError::bad_request(format!(
                    "执行层 Agent 请求了未授权工具 {name}"
                )));
            }
            let arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments = serde_json::from_str(arguments)
                .map_err(|_| AppError::bad_request("执行层工具参数不是合法 JSON"))?;
            let request = toonflow_agent_tools::ToolRequest {
                emitter: None,
                agent_type: "productionAgent".to_string(),
                project_id,
                script_id,
                tool_name: name.to_string(),
                arguments,
            };
            let result = Box::pin(toonflow_agent_tools::execute_recorded(state, &request)).await;
            let content = match result {
                Ok((_, value)) => value.to_string(),
                Err(error) => json!({"error":format!("{error:?}")}).to_string(),
            };
            messages.push(json!({"role":"tool","tool_call_id":call_id,"content":content}));
        }
    }
    Err(AppError::bad_request("执行层 Agent 工具调用超过最大轮数"))
}

async fn run_native_tools(
    state: &ToonState,
    request: &ChatRequest,
    agent_key: &str,
    system: &str,
    run_id: i64,
) -> Result<String, String> {
    let mut messages = vec![
        json!({"role":"system","content":system}),
        json!({"role":"user","content":request.content}),
    ];
    for _ in 0..12 {
        let raw = ai_client::text_tools(
            &state.pool,
            agent_key,
            messages.clone(),
            native_tool_definitions(&request.agent_type),
        )
        .await?;
        let message = raw
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| "模型工具响应缺少 message".to_string())?;
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            let output = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if output.is_empty() {
                return Err("模型工具响应缺少文本".into());
            }
            sqlx::query("INSERT INTO toonflow.agent_run_events(run_id,event_type,data,create_time)VALUES($1,'delta',$2,$3)")
                .bind(run_id)
                .bind(json!({"text":output}))
                .bind(now_ms())
                .execute(&state.pool)
                .await
                .map_err(|error| error.to_string())?;
            return Ok(output);
        }
        messages.push(message);
        for call in calls {
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool_call 缺少 id".to_string())?;
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool_call 缺少名称".to_string())?;
            if !tool_names(&request.agent_type).contains(&name) {
                return Err(format!("模型请求了未授权工具 {name}"));
            }
            let arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments: Value =
                serde_json::from_str(arguments).map_err(|_| "工具参数不是合法 JSON".to_string())?;
            let tool_request = toonflow_agent_tools::ToolRequest {
                emitter: None,
                agent_type: request.agent_type.clone(),
                project_id: request.project_id,
                script_id: request.script_id,
                tool_name: name.to_string(),
                arguments,
            };
            let output = match toonflow_agent_tools::execute_recorded(state, &tool_request).await {
                Ok((_, value)) => value,
                Err(error) => json!({"error":format!("{error:?}")}),
            };
            messages
                .push(json!({"role":"tool","tool_call_id":call_id,"content":output.to_string()}));
        }
    }
    Err("Agent 工具调用超过最大轮数".into())
}

async fn run_with_tools(
    state: &ToonState,
    request: &ChatRequest,
    agent_key: &str,
    system: &str,
    run_id: i64,
) -> Result<String, String> {
    let dynamic_skills = toonflow_agent_runtime::dynamic_skills(&state.pool).await?;
    let skill_guide = if dynamic_skills.is_empty() {
        String::new()
    } else {
        format!(
            "\n动态 Skill：{}。需要专项技法时调用 use_skill({{path}})。",
            dynamic_skills
                .iter()
                .map(|(path, name)| format!("{name}={path}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let complete_system = format!(
        "{system}\n\n{}{}\n{}",
        tool_guide(&request.agent_type),
        skill_guide,
        thinking_instruction(request.think, request.think_level)
    );
    match run_native_tools(state, request, agent_key, &complete_system, run_id).await {
        Ok(output) => return Ok(output),
        Err(error) if !error.contains("暂不支持工具调用") => return Err(error),
        Err(_) => {}
    }
    let mut prompt = request.content.clone();
    for _ in 0..4 {
        let event_pool = state.pool.clone();
        let output = ai_client::text_stream(
            &state.pool,
            agent_key,
            &complete_system,
            &prompt,
            move |delta| { let pool=event_pool.clone(); async move { sqlx::query("INSERT INTO toonflow.agent_run_events(run_id,event_type,data,create_time)VALUES($1,'delta',$2,$3)").bind(run_id).bind(json!({"text":delta})).bind(now_ms()).execute(&pool).await.map_err(|error|error.to_string())?;Ok(()) } },
        )
        .await?;
        let calls = parse_tool_calls(&output);
        if calls.is_empty() {
            return Ok(output);
        }
        let mut results = Vec::new();
        for (name, arguments) in calls {
            let tool_request = toonflow_agent_tools::ToolRequest {
                emitter: None,
                agent_type: request.agent_type.clone(),
                project_id: request.project_id,
                script_id: request.script_id,
                tool_name: name.clone(),
                arguments,
            };
            match toonflow_agent_tools::execute_recorded(state, &tool_request).await {
                Ok((call_id, value)) => results
                    .push(json!({"callId":call_id,"tool":name,"success":true,"result":value})),
                Err(error) => {
                    results.push(json!({"tool":name,"success":false,"error":format!("{error:?}")}))
                }
            }
        }
        prompt = format!(
            "用户任务：{}\n工具执行结果：{}\n请根据结果继续；如无需其他工具，直接给出最终答复。",
            request.content,
            serde_json::to_string_pretty(&results).unwrap_or_default()
        );
    }
    Err("Agent 工具调用超过最大轮数".to_string())
}

async fn create_run(state: &ToonState, request: &ChatRequest) -> Result<i64, AppError> {
    validate_agent(&request.agent_type)?;
    if request.isolation_key.trim().is_empty() || request.content.trim().is_empty() {
        return Err(AppError::bad_request("isolationKey 和 content 不能为空"));
    }
    let run_id = next_id(0);
    sqlx::query("INSERT INTO toonflow.agent_runs(id,agent_type,isolation_key,project_id,script_id,input,state,think,think_level,start_time) VALUES($1,$2,$3,$4,$5,$6,'running',$7,$8,$9)").bind(run_id).bind(&request.agent_type).bind(&request.isolation_key).bind(request.project_id).bind(request.script_id).bind(&request.content).bind(request.think).bind(request.think_level.clamp(0,3)).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to start agent run"))?;
    Ok(run_id)
}

async fn perform_run(
    state: &ToonState,
    request: &ChatRequest,
    run_id: i64,
) -> Result<String, String> {
    let agent_key = validate_agent(&request.agent_type).map_err(|error| format!("{error:?}"))?;
    add_memory(
        state,
        &request.agent_type,
        &request.isolation_key,
        "user",
        &request.content,
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    let context = project_context(state, request)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let memory = memory_context(state, request)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let skill = toonflow_agent_runtime::load_agent_skill(&state.pool, agent_key).await?;
    let system = format!("{skill}\n\n{context}\n{memory}");
    match run_with_tools(state, request, agent_key, &system, run_id).await {
        Ok(output) => {
            add_memory(
                state,
                &request.agent_type,
                &request.isolation_key,
                "assistant",
                &output,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
            sqlx::query("UPDATE toonflow.agent_runs SET output=$2,state='success',finish_time=$3 WHERE id=$1 AND state='running'").bind(run_id).bind(&output).bind(now_ms()).execute(&state.pool).await.map_err(|error|error.to_string())?;
            summarize_if_needed(state, request, agent_key).await;
            Ok(output)
        }
        Err(error) => {
            sqlx::query("UPDATE toonflow.agent_runs SET state='failed',error_reason=$2,finish_time=$3 WHERE id=$1 AND state='running'").bind(run_id).bind(&error).bind(now_ms()).execute(&state.pool).await.ok();
            Err(error)
        }
    }
}

/// Run the agent with a WebSocket emitter for real-time streaming.
/// Uses the proven run_with_tools() logic and visualizes the output.
pub(crate) async fn run_with_emitter(
    state: &ToonState,
    request: &ChatRequest,
    agent_key: &str,
    emitter: &WsEmitter,
    msg_id: &str,
    text_cid: &str,
    mut abort_rx: watch::Receiver<bool>,
) -> Result<String, String> {
    // Add user memory
    let _ = add_memory(
        state,
        &request.agent_type,
        &request.isolation_key,
        "user",
        &request.content,
    )
    .await;

    let context = project_context(state, request)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let memory = memory_context(state, request)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let skill = toonflow_agent_runtime::load_agent_skill(&state.pool, agent_key).await?;
    let dynamic_skills = toonflow_agent_runtime::dynamic_skills(&state.pool).await?;

    let skill_guide = if dynamic_skills.is_empty() {
        String::new()
    } else {
        format!(
            "\n动态 Skill：{}。需要专项技法时调用 use_skill({{path}})。",
            dynamic_skills
                .iter()
                .map(|(path, name)| format!("{name}={path}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let pipeline_rule = if request.agent_type == "scriptAgent" {
        "\n\n## 流水线铁律\n1. 阶段必须串行：故事骨架 → 改编策略 → 剧本编写，禁止跳过或合并\n2. 每阶段完成后必须调用 run_supervision_agent 审核，审核报告展示给用户\n3. 用户确认后才能进入下一阶段\n4. 阶段3剧本编写：必须为每一集单独调用 run_sub_agent_script，每调用一次只生成一集。全部集数完成后统一调用 save_scripts 写入。禁止提前结束，必须生成完所有集数。"
    } else {
        ""
    };
    let system = format!(
        "{skill}\n\n{context}\n{memory}\n{}{}\n{}{pipeline_rule}",
        tool_guide(&request.agent_type),
        skill_guide,
        thinking_instruction(request.think, request.think_level),
    );

    // Try native tools first (handles multi-turn tool calls properly)
    let mut messages = vec![
        json!({"role":"system","content":system}),
        json!({"role":"user","content":request.content}),
    ];

    for round in 0..12 {
        if *abort_rx.borrow_and_update() {
            return Err("用户已中止".into());
        }

        let raw = ai_client::text_tools(
            &state.pool,
            agent_key,
            messages.clone(),
            native_tool_definitions(&request.agent_type),
        )
        .await?;

        let message = raw
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| "模型工具响应缺少 message".to_string())?;

        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if calls.is_empty() {
            let text = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if text.is_empty() {
                // Model might have hit context limit or returned refusal
                // Continue loop to retry with shorter context
                warn!("模型返回空内容，跳过本轮（第{}轮）", round);
                continue;
            }

            // Stream text with think tag detection
            let open_tag = "<think>";
            let close_tag = "</think>";
            let mut rest = text.as_str();
            let mut in_thinking = false;
            let mut thinking_cid: Option<String> = None;
            let mut thinking_start = 0i64;
            let mut thinking_buf = String::new();

            while !rest.is_empty() {
                if !in_thinking {
                    if let Some(idx) = rest.find(open_tag) {
                        if idx > 0 {
                            emitter.text_delta(msg_id, text_cid, &rest[..idx]);
                        }
                        in_thinking = true;
                        thinking_start = now_ms();
                        thinking_buf.clear();
                        thinking_cid = Some(emitter.thinking_start(msg_id, "思考中..."));
                        rest = &rest[idx + open_tag.len()..];
                    } else {
                        emitter.text_delta(msg_id, text_cid, rest);
                        break;
                    }
                } else {
                    if let Some(idx) = rest.find(close_tag) {
                        let think_text = &rest[..idx];
                        thinking_buf.push_str(think_text);
                        if let Some(ref cid) = thinking_cid {
                            let elapsed = (now_ms() - thinking_start) as f64 / 1000.0;
                            let title = format!("思考完毕（{elapsed:.1}秒）");
                            emitter.thinking_complete(msg_id, cid, &title, &thinking_buf);
                        }
                        thinking_cid = None;
                        in_thinking = false;
                        rest = &rest[idx + close_tag.len()..];
                    } else {
                        thinking_buf.push_str(rest);
                        if let Some(ref cid) = thinking_cid {
                            emitter.thinking_append(msg_id, cid, rest);
                        }
                        break;
                    }
                }
            }

            let _ = add_memory(
                state,
                &request.agent_type,
                &request.isolation_key,
                "assistant",
                &text,
            )
            .await;
            return Ok(text);
        }

        // Process tool calls
        messages.push(message);
        for call in &calls {
            let call_id = call.get("id").and_then(Value::as_str).unwrap_or("");
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments: Value = serde_json::from_str(arguments).unwrap_or(json!({}));

            // Visualize tool call
            let tool_call_id = uuid::Uuid::new_v4().to_string();
            let args_str = serde_json::to_string_pretty(&arguments).unwrap_or_default();
            let tc_cid = emitter.tool_call_start(msg_id, &tool_call_id, name);
            emitter.tool_call_args(msg_id, &tc_cid, &tool_call_id, &args_str);

            if !tool_names(&request.agent_type).contains(&name) {
                let err = format!("模型请求了未授权工具 {name}");
                emitter.tool_call_error(msg_id, &tc_cid, &tool_call_id, &err);
                return Err(err);
            }

            let tool_request = toonflow_agent_tools::ToolRequest {
                agent_type: request.agent_type.clone(),
                project_id: request.project_id,
                script_id: request.script_id,
                tool_name: name.to_string(),
                arguments,
                emitter: Some(emitter.clone()),
            };

            match toonflow_agent_tools::execute_recorded(state, &tool_request).await {
                Ok((_, value)) => {
                    let result_str = serde_json::to_string_pretty(&value).unwrap_or_default();
                    emitter.tool_call_success(msg_id, &tc_cid, &tool_call_id, &result_str);
                    messages.push(
                        json!({"role":"tool","tool_call_id":call_id,"content":value.to_string()}),
                    );
                }
                Err(error) => {
                    let err_str = format!("{error:?}");
                    emitter.tool_call_error(msg_id, &tc_cid, &tool_call_id, &err_str);
                    messages.push(json!({"role":"tool","tool_call_id":call_id,"content":err_str}));
                }
            }
        }
    }

    Err("Agent 工具调用超过最大轮数".into())
}

pub async fn chat(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    recover_stale(&state).await;
    let run_id = create_run(&state, &request).await?;
    let output = perform_run(&state, &request, run_id)
        .await
        .map_err(AppError::bad_request)?;
    Ok(Json(ApiResponse::new(
        json!({"id":run_id,"state":"success","content":output}),
    )))
}

pub async fn start(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    recover_stale(&state).await;
    let run_id = create_run(&state, &request).await?;
    let task_state = state.clone();
    let handle = tokio::spawn(async move {
        let _ = perform_run(&task_state, &request, run_id).await;
        ACTIVE_RUNS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&run_id);
    });
    ACTIVE_RUNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(run_id, handle.abort_handle());
    Ok(Json(ApiResponse::new(
        json!({"id":run_id,"state":"running"}),
    )))
}

pub async fn run_state(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<RunIdRequest>,
) -> Result<Json<ApiResponse<RunRow>>, AppError> {
    require(&user, "toon:project:read")?;
    let row=sqlx::query_as("SELECT id,agent_type,isolation_key,project_id,script_id,input,output,state,error_reason,think,think_level,start_time,finish_time FROM toonflow.agent_runs WHERE id=$1").bind(request.id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to get agent run"))?.ok_or_else(||AppError::not_found("agent run not found"))?;
    Ok(Json(ApiResponse::new(row)))
}

pub async fn stop(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<RunIdRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    if let Some(handle) = ACTIVE_RUNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&request.id)
    {
        handle.abort();
    }
    let result=sqlx::query("UPDATE toonflow.agent_runs SET state='canceled',error_reason='用户已中止',finish_time=$2 WHERE id=$1 AND state='running'").bind(request.id).bind(now_ms()).execute(&state.pool).await.map_err(|_|AppError::internal("failed to stop agent run"))?;
    if result.rows_affected() == 0 {
        return Err(AppError::bad_request("运行已结束或不存在"));
    }
    Ok(Json(ApiResponse::new(
        json!({"id":request.id,"state":"canceled"}),
    )))
}

pub async fn events(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<EventRequest>,
) -> Result<Json<ApiResponse<Vec<RunEvent>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows=sqlx::query_as("SELECT id,run_id,event_type,data,create_time FROM toonflow.agent_run_events WHERE run_id=$1 AND id>$2 ORDER BY id LIMIT 500").bind(request.run_id).bind(request.after_id).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to get agent events"))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn recover_stale(state: &ToonState) {
    let active = ACTIVE_RUNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .keys()
        .copied()
        .collect::<Vec<_>>();
    let _=sqlx::query("UPDATE toonflow.agent_runs SET state='interrupted',error_reason='服务重启导致任务中断',finish_time=$1 WHERE state='running' AND NOT(id=ANY($2))").bind(now_ms()).bind(active).execute(&state.pool).await;
}

pub async fn retry(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(source): Json<RunIdRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    recover_stale(&state).await;
    let row:Option<(String,String,i64,Option<i64>,String,bool,i32)>=sqlx::query_as("SELECT agent_type,isolation_key,project_id,script_id,input,think,think_level FROM toonflow.agent_runs WHERE id=$1 AND state IN('failed','canceled','interrupted')").bind(source.id).fetch_optional(&state.pool).await.map_err(|_|AppError::internal("failed to load retry run"))?;
    let (agent_type, isolation_key, project_id, script_id, content, think, think_level) =
        row.ok_or_else(|| AppError::bad_request("仅失败、中止或中断的运行可重试"))?;
    let request = ChatRequest {
        agent_type,
        isolation_key,
        project_id,
        script_id,
        content,
        think,
        think_level,
    };
    let run_id = create_run(&state, &request).await?;
    let task_state = state.clone();
    let handle = tokio::spawn(async move {
        let _ = perform_run(&task_state, &request, run_id).await;
        ACTIVE_RUNS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&run_id);
    });
    ACTIVE_RUNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(run_id, handle.abort_handle());
    Ok(Json(ApiResponse::new(
        json!({"id":run_id,"state":"running","retryOf":source.id}),
    )))
}

pub async fn memories(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SessionRequest>,
) -> Result<Json<ApiResponse<Vec<MemoryRow>>>, AppError> {
    require(&user, "toon:project:read")?;
    validate_agent(&request.agent_type)?;
    let rows=sqlx::query_as("SELECT id,role,content,memory_type,create_time FROM toonflow.agent_memories WHERE agent_type=$1 AND isolation_key=$2 ORDER BY create_time").bind(request.agent_type).bind(request.isolation_key).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list memories"))?;
    Ok(Json(ApiResponse::new(rows)))
}
pub async fn runs(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<SessionRequest>,
) -> Result<Json<ApiResponse<Vec<RunRow>>>, AppError> {
    require(&user, "toon:project:read")?;
    validate_agent(&request.agent_type)?;
    let rows=sqlx::query_as("SELECT id,agent_type,isolation_key,project_id,script_id,input,output,state,error_reason,think,think_level,start_time,finish_time FROM toonflow.agent_runs WHERE agent_type=$1 AND isolation_key=$2 ORDER BY start_time DESC LIMIT 50").bind(request.agent_type).bind(request.isolation_key).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list agent runs"))?;
    Ok(Json(ApiResponse::new(rows)))
}
pub async fn clear(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(request): Json<ClearMemoryRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:project:update")?;
    validate_agent(&request.agent_type)?;
    let memory_type = request.memory_type.as_deref().unwrap_or("all");
    if !matches!(memory_type, "message" | "summary" | "all") {
        return Err(AppError::bad_request(
            "memoryType 仅支持 message、summary 或 all",
        ));
    }
    sqlx::query("DELETE FROM toonflow.agent_memories WHERE agent_type=$1 AND isolation_key=$2 AND ($3='all' OR ($3='summary' AND memory_type='summary') OR ($3='message' AND memory_type<>'summary'))")
        .bind(request.agent_type)
        .bind(request.isolation_key)
        .bind(memory_type)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to clear memories"))?;
    Ok(Json(ApiResponse::new(json!(true))))
}

#[cfg(test)]
mod tests {
    use super::parse_tool_calls;

    #[test]
    fn parses_multiple_tool_calls() {
        let calls = parse_tool_calls(
            r#"<tool_call>{"name":"get_flowData","arguments":{"key":"scriptPlan"}}</tool_call><tool_call>{"name":"generate_storyboard","arguments":{"ids":[1,2]}}</tool_call>"#,
        );
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "get_flowData");
        assert_eq!(calls[1].1["ids"], serde_json::json!([1, 2]));
    }

    #[test]
    fn ignores_invalid_tool_payload() {
        assert!(parse_tool_calls("<tool_call>not-json</tool_call>").is_empty());
    }
}
