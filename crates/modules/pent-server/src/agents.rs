//! Markdown agent profile management — CRUD over the `PENTA_AGENTS_DIR`
//! directory of front-matter markdown files used by `agent_profiles`.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};

use crate::conversation::agent_profiles;
use crate::{ApiResponse, PentState, security};

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/agents", get(list_agents).post(create_agent))
        .route(
            "/pent/agents/{id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
}

fn profile_json(profile: &agent_profiles::AgentProfile) -> Value {
    json!({
        "id": profile.id,
        "name": profile.name,
        "description": profile.description,
        "instruction": profile.instruction,
    })
}

fn valid_id(id: &str) -> Result<&str, AppError> {
    let id = id.trim();
    if agent_profiles::is_valid_id(id) {
        Ok(id)
    } else {
        Err(AppError::bad_request(
            "id 不能为空且只能包含字母、数字、中划线和下划线",
        ))
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::internal(error.to_string())
}

async fn list_agents(
    State(_s): State<PentState>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let dir = agent_profiles::agents_dir();
    let mut agents = Vec::new();
    if dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .map_err(io_error)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect();
        entries.sort();
        for path in entries {
            let fallback_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            match agent_profiles::parse(&path, fallback_id) {
                Ok(profile) => agents.push(profile_json(&profile)),
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "skipping unparsable agent profile")
                }
            }
        }
    }
    Ok(Json(ApiResponse::new(json!({
        "dir": dir.to_string_lossy(),
        "agents": agents,
    }))))
}

async fn get_agent(
    State(_s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let id = valid_id(&id)?;
    let profile = agent_profiles::load(id)
        .map_err(|error| AppError::internal(error.to_string()))?
        .ok_or_else(|| AppError::not_found("智能体不存在"))?;
    Ok(Json(ApiResponse::new(profile_json(&profile))))
}

fn render_markdown(id: &str, name: &str, description: &str, instruction: &str) -> String {
    format!("---\nid: {id}\nname: {name}\ndescription: {description}\n---\n\n{}\n", instruction.trim())
}

async fn create_agent(
    State(_s): State<PentState>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let id = valid_id(body.get("id").and_then(Value::as_str).unwrap_or(""))?;
    let name = body.get("name").and_then(Value::as_str).unwrap_or(id);
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let instruction = body
        .get("instruction")
        .and_then(Value::as_str)
        .unwrap_or("");
    let dir = agent_profiles::agents_dir();
    std::fs::create_dir_all(&dir).map_err(io_error)?;
    let path = dir.join(format!("{id}.md"));
    if path.exists() {
        return Err(AppError::bad_request("智能体 id 已存在"));
    }
    std::fs::write(&path, render_markdown(id, name, description, instruction))
        .map_err(io_error)?;
    Ok(Json(ApiResponse::new(json!({
        "id": id,
        "name": name,
        "description": description,
        "instruction": instruction.trim(),
    }))))
}

async fn update_agent(
    State(_s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let id = valid_id(&id)?;
    let dir = agent_profiles::agents_dir();
    let path = dir.join(format!("{id}.md"));
    if !path.exists() {
        return Err(AppError::not_found("智能体不存在"));
    }
    let name = body.get("name").and_then(Value::as_str).unwrap_or(id);
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let instruction = body
        .get("instruction")
        .and_then(Value::as_str)
        .unwrap_or("");
    std::fs::write(&path, render_markdown(id, name, description, instruction))
        .map_err(io_error)?;
    Ok(Json(ApiResponse::new(json!({
        "id": id,
        "name": name,
        "description": description,
        "instruction": instruction.trim(),
    }))))
}

async fn delete_agent(
    State(_s): State<PentState>,
    Path(id): Path<String>,
    user: CurrentUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let _tenant_id = security::tenant_id(&user)?;
    let id = valid_id(&id)?;
    let path = agent_profiles::agents_dir().join(format!("{id}.md"));
    if !path.exists() {
        return Err(AppError::not_found("智能体不存在"));
    }
    std::fs::remove_file(&path).map_err(io_error)?;
    Ok(Json(ApiResponse::new(json!({"deleted": true}))))
}
