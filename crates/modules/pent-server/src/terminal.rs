use std::{collections::HashMap, process::Stdio, sync::OnceLock};

use axum::{extract::{Path, Query}, routing::{delete, get, post}, Json, Router};
use kairos_platform_framework_security::CurrentUser;
use kairos_platform_framework_web::AppError;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, process::{Child, ChildStdin, Command}, sync::Mutex};
use uuid::Uuid;

use crate::{ApiResponse, PentState, security};

struct Session { child: Child, input: ChildStdin, owner: String, output: Vec<(u64,String)>, sequence: u64 }
static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
fn sessions() -> &'static Mutex<HashMap<String, Session>> { SESSIONS.get_or_init(|| Mutex::new(HashMap::new())) }

pub fn routes() -> Router<PentState> {
    Router::new()
        .route("/pent/settings/terminal/sessions", post(create_session))
        .route("/pent/settings/terminal/sessions/{id}", delete(close_session))
        .route("/pent/settings/terminal/sessions/{id}/input", post(write_input))
        .route("/pent/settings/terminal/sessions/{id}/output", get(read_output))
}

async fn create_session(user: CurrentUser) -> Result<Json<ApiResponse<Value>>, AppError> {
    security::require(&user,"pent:tool:manage")?;
    let id=Uuid::new_v4().to_string();
    let shell_command = r"exec env PS1='(base) ┌──(\u㉿\h)-[\w]\n└─\$ ' bash --noprofile --norc -i";
    let mut child=Command::new("script").args(["-qefc",shell_command,"/dev/null"])
        .env("TERM","xterm-256color")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()
        .map_err(|e|AppError::internal(format!("启动终端失败: {e}")))?;
    let input=child.stdin.take().ok_or_else(||AppError::internal("终端输入不可用"))?;
    let mut output=child.stdout.take().ok_or_else(||AppError::internal("终端输出不可用"))?;
    sessions().lock().await.insert(id.clone(),Session{child,input,owner:user.user_id,output:Vec::new(),sequence:0});
    let reader_id=id.clone();
    tokio::spawn(async move { let mut buffer=[0_u8;8192]; loop { match output.read(&mut buffer).await { Ok(0)|Err(_)=>break, Ok(size)=>{ let text=String::from_utf8_lossy(&buffer[..size]).to_string(); let mut all=sessions().lock().await;if let Some(session)=all.get_mut(&reader_id){session.sequence+=1;let sequence=session.sequence;session.output.push((sequence,text));if session.output.len()>1000{session.output.drain(..500);}}else{break} } } } });
    Ok(Json(ApiResponse::new(json!({"id":id}))))
}

async fn write_input(Path(id):Path<String>,user:CurrentUser,Json(body):Json<Value>)->Result<Json<ApiResponse<Value>>,AppError>{security::require(&user,"pent:tool:manage")?;let data=body["data"].as_str().ok_or_else(||AppError::bad_request("data 不能为空"))?;let mut all=sessions().lock().await;let session=all.get_mut(&id).ok_or_else(||AppError::not_found("终端会话不存在"))?;if session.owner!=user.user_id{return Err(AppError::forbidden("无权访问该终端"));}session.input.write_all(data.as_bytes()).await.map_err(|e|AppError::internal(e.to_string()))?;session.input.flush().await.map_err(|e|AppError::internal(e.to_string()))?;Ok(Json(ApiResponse::new(json!({"ok":true}))))}

#[derive(Deserialize)] struct OutputQuery{after:Option<u64>}
async fn read_output(Path(id):Path<String>,Query(query):Query<OutputQuery>,user:CurrentUser)->Result<Json<ApiResponse<Value>>,AppError>{let all=sessions().lock().await;let session=all.get(&id).ok_or_else(||AppError::not_found("终端会话不存在"))?;if session.owner!=user.user_id{return Err(AppError::forbidden("无权访问该终端"));}let after=query.after.unwrap_or(0);let chunks:Vec<Value>=session.output.iter().filter(|(seq,_)|*seq>after).map(|(seq,data)|json!({"sequence":seq,"data":data})).collect();Ok(Json(ApiResponse::new(json!({"chunks":chunks,"sequence":session.sequence}))))}

async fn close_session(Path(id):Path<String>,user:CurrentUser)->Result<Json<ApiResponse<Value>>,AppError>{let mut all=sessions().lock().await;let session=all.get(&id).ok_or_else(||AppError::not_found("终端会话不存在"))?;if session.owner!=user.user_id{return Err(AppError::forbidden("无权访问该终端"));}let mut session=all.remove(&id).expect("checked");let _=session.child.kill().await;Ok(Json(ApiResponse::new(json!({"closed":true}))))}
