use axum::{Json, extract::State};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};

use crate::{ToonState, ai_client, shared::require};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    project_id: i64,
    novel_ids: Vec<i64>,
    concurrent_count: Option<usize>,
}
pub async fn generate(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<GenerateRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    require(&user, "toon:project:update")?;
    if req.novel_ids.is_empty() {
        return Err(AppError::bad_request("没有对应章节"));
    }
    sqlx::query("UPDATE toonflow.novels SET event_state=0,event=NULL,error_reason=NULL WHERE project_id=$1 AND id=ANY($2)").bind(req.project_id).bind(&req.novel_ids).execute(&state.pool).await.map_err(|_|AppError::internal("failed to reset novel event state"))?;
    let pool = state.pool.clone();
    let project_id = req.project_id;
    let ids = req.novel_ids;
    let concurrency = req.concurrent_count.unwrap_or(5).clamp(1, 20);
    tokio::spawn(async move {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut jobs = Vec::new();
        for id in ids {
            let pool = pool.clone();
            let permit = semaphore.clone().acquire_owned().await;
            jobs.push(tokio::spawn(async move {
                if permit.is_ok() {
                    process_chapter(&pool, project_id, id).await;
                }
            }));
        }
        for job in jobs {
            let _ = job.await;
        }
    });
    Ok(Json(ApiResponse::new("生成事件成功")))
}

async fn process_chapter(pool: &PgPool, project_id: i64, id: i64) {
    let chapter: Option<(String, String)> = sqlx::query_as(
        "SELECT chapter,chapter_data FROM toonflow.novels WHERE id=$1 AND project_id=$2",
    )
    .bind(id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let Some((title, content)) = chapter else {
        return;
    };
    let task_id = chrono::Utc::now().timestamp_millis() * 1_000_000 + id % 1_000_000;
    let _=sqlx::query("INSERT INTO toonflow.tasks(id,project_id,task_class,related_objects,model,description,state,start_time) VALUES($1,$2,'novelEvent',$3,'universalAi',$4,'running',$5) ON CONFLICT(id) DO NOTHING").bind(task_id).bind(project_id).bind(id.to_string()).bind(format!("提取事件：{title}")).bind(chrono::Utc::now().timestamp_millis()).execute(pool).await;
    // Try loading custom prompt from prompts table, fall back to built-in
    let system = sqlx::query_scalar::<_,String>(
        "SELECT data FROM toonflow.prompts WHERE type='eventExtraction' AND use_data=true ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| r#"你是小说文本分析助手。用户每次提供一个章节的原文，你提取该章的结构化事件信息。

## ⚠️ 输出约束（最高优先级，违反任何一条即为失败）
1. 只输出纯 JSON 数组，第一个字符必须是 `[`
2. 不输出任何引导语、解释、总结、Markdown、代码块标记
3. 不输出表头行、分隔线、emoji

## 输出格式
[
  {"name":"15字以内事件名","detail":"核心事件描述","characters":"涉及角色","mainline":"强/中/弱（理由）","density":"高/中/低","duration":"X秒","mood":"情绪标签"}
]

## 字段规范
- name：动作+结果，15字以内，禁止笼统（如"主角经历了一些事"）
- detail：30-60字，必须含：谁做了什么→什么结果→对主线的影响
- characters：有实际戏份的角色名，顿号分隔
- mainline：强/中/弱 + （3-8字理由）。强=直接推动主角弧线；中=补充世界观/人物关系/伏笔；弱=过渡/气氛
- density：高/中/低。高密度+高情绪→45-60秒；中→35-45秒；低→25-35秒
- duration：预估集长，格式为"X秒"，禁止用分钟
- mood：从[冲突/情感/转折/高潮/悬疑/平铺/喜剧/爽感/压抑/期待/震撼/温馨]中选

## 提取规则
- 忠于原文，不推测不脑补，不加入原文未出现的情节
- 多条平行事件线时，选对主角影响最大的，其余简要带过
- 对话密集章节，关注对话推动了什么结果，而非复述对话内容
- 每3000字提取3-5个事件，短章节至少2个"#.to_string());
    match ai_client::text(
        pool,
        "universalAi",
        &system,
        &format!("章节：{title}\n\n{content}"),
    )
    .await
    {
        Ok(raw) => {
            if let Err(reason) = save_events(pool, id, &raw).await {
                fail(pool, id, task_id, &reason).await
            } else {
                let _ = sqlx::query("UPDATE toonflow.tasks SET state='success' WHERE id=$1")
                    .bind(task_id)
                    .execute(pool)
                    .await;
            }
        }
        Err(reason) => fail(pool, id, task_id, &reason).await,
    }
}
async fn save_events(pool: &PgPool, novel_id: i64, raw: &str) -> Result<(), String> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let events: Vec<Value> =
        serde_json::from_str(cleaned).map_err(|e| format!("事件 JSON 解析失败: {e}"))?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Delete old events for this novel (IDs are novel_id * 1000 + 0..999)
    let id_start = novel_id * 1000;
    let id_end = novel_id * 1000 + 999;
    sqlx::query("DELETE FROM toonflow.event_chapters WHERE novel_id=$1")
        .bind(novel_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM toonflow.events WHERE id BETWEEN $1 AND $2")
        .bind(id_start)
        .bind(id_end)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for (index, event) in events.iter().enumerate() {
        let name = event
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("未命名事件");
        let detail = event.get("detail").and_then(Value::as_str).unwrap_or("");
        let event_id = novel_id * 1000 + index as i64;
        sqlx::query("INSERT INTO toonflow.events(id,name,detail,create_time) VALUES($1,$2,$3,$4)")
            .bind(event_id)
            .bind(name)
            .bind(detail)
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO toonflow.event_chapters(event_id,novel_id) VALUES($1,$2)")
            .bind(event_id)
            .bind(novel_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    sqlx::query("UPDATE toonflow.novels SET event=$2,event_state=1,error_reason=NULL WHERE id=$1")
        .bind(novel_id)
        .bind(raw)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}
async fn fail(pool: &PgPool, id: i64, task_id: i64, reason: &str) {
    let _ = sqlx::query("UPDATE toonflow.novels SET event_state=-1,error_reason=$2 WHERE id=$1")
        .bind(id)
        .bind(reason)
        .execute(pool)
        .await;
    let _ = sqlx::query("UPDATE toonflow.tasks SET state='failed',reason=$2 WHERE id=$1")
        .bind(task_id)
        .bind(reason)
        .execute(pool)
        .await;
}

#[derive(Deserialize)]
pub struct StatesRequest {
    ids: Vec<i64>,
}
pub async fn states(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<StatesRequest>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    require(&user, "toon:project:read")?;
    let rows=sqlx::query_as::<_,(i64,Option<String>,i32,Option<String>)>("SELECT id,event,event_state,error_reason FROM toonflow.novels WHERE id=ANY($1) AND event_state<>0").bind(req.ids).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list event states"))?;
    Ok(Json(ApiResponse::new(
        rows.into_iter()
            .map(|r| json!({"id":r.0,"event":r.1,"eventState":r.2,"errorReason":r.3}))
            .collect(),
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListRequest {
    project_id: i64,
    page: i64,
    limit: i64,
    search: Option<String>,
}
#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct EventRow {
    id: i64,
    event_name: String,
    detail: String,
    create_time: i64,
    chapters: Vec<i32>,
}
pub async fn list(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<ListRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require(&user, "toon:episode:read")?;
    let search = req.search.filter(|s| !s.is_empty());
    let total:(i64,)=sqlx::query_as("SELECT count(DISTINCT e.id) FROM toonflow.events e JOIN toonflow.event_chapters ec ON ec.event_id=e.id JOIN toonflow.novels n ON n.id=ec.novel_id WHERE n.project_id=$1 AND ($2::text IS NULL OR e.name ILIKE '%'||$2||'%')").bind(req.project_id).bind(&search).fetch_one(&state.pool).await.map_err(|_|AppError::internal("failed to count events"))?;
    let rows=sqlx::query_as::<_,EventRow>("SELECT e.id,e.name event_name,e.detail,e.create_time,array_agg(n.chapter_index ORDER BY n.chapter_index)::int4[] chapters FROM toonflow.events e JOIN toonflow.event_chapters ec ON ec.event_id=e.id JOIN toonflow.novels n ON n.id=ec.novel_id WHERE n.project_id=$1 AND ($2::text IS NULL OR e.name ILIKE '%'||$2||'%') GROUP BY e.id ORDER BY e.id DESC OFFSET $3 LIMIT $4").bind(req.project_id).bind(search).bind((req.page.max(1)-1)*req.limit.clamp(1,100)).bind(req.limit.clamp(1,100)).fetch_all(&state.pool).await.map_err(|_|AppError::internal("failed to list events"))?;
    Ok(Json(ApiResponse::new(json!({"list":rows,"total":total.0}))))
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    id: i64,
}
#[derive(Deserialize)]
pub struct BatchDeleteRequest {
    ids: Vec<i64>,
}
pub async fn delete(
    user: CurrentUser,
    state: State<ToonState>,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    batch_delete(user, state, Json(BatchDeleteRequest { ids: vec![req.id] })).await
}
pub async fn batch_delete(
    user: CurrentUser,
    State(state): State<ToonState>,
    Json(req): Json<BatchDeleteRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    require(&user, "toon:episode:delete")?;
    sqlx::query("DELETE FROM toonflow.events WHERE id=ANY($1)")
        .bind(req.ids)
        .execute(&state.pool)
        .await
        .map_err(|_| AppError::internal("failed to delete events"))?;
    Ok(Json(ApiResponse::new(())))
}
