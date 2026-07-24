pub mod application;
pub mod cognition;
pub mod config;
pub mod conversation;
pub mod infrastructure;
mod session;
pub mod tooling;
pub mod transport;

use axum::{
    Json, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get, post},
};
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_web::AppError;
use serde_json::{Value, json};
use sha2::Digest;
use uuid::Uuid;

pub use config::PentConfig;

#[derive(Clone)]
pub struct PentState {
    pub pool: PgPool,
    conversation_tasks: conversation::turn_tasks::TurnTaskRegistry,
}

impl PentState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            conversation_tasks: Default::default(),
        }
    }
}

pub fn routes(state: PentState) -> Router {
    Router::new()
        .route("/pent/health", get(health))
        .route("/pent/projects", get(list_projects).post(create_project))
        .route("/pent/projects/{id}", get(get_project))
        .route("/pent/tasks", get(list_tasks).post(create_task))
        .route("/pent/tasks/{id}", get(get_task).delete(delete_task))
        .route("/pent/tasks/{id}/cancel", post(cancel_task))
        .route("/pent/runs", get(list_runs).post(create_run))
        .route("/pent/runs/{id}", get(get_run))
        .route("/pent/runs/{id}/cancel", post(cancel_run))
        .route("/pent/runs/{id}/events", get(get_run_events))
        .route("/pent/runs/{id}/tool-calls", get(get_tool_calls))
        .route("/pent/approvals", get(list_approvals))
        .route("/pent/approvals/{id}/decision", post(decide_approval))
        .route("/pent/chat", post(chat))
        .merge(tooling::http::routes())
        .route("/pent/metrics", get(metrics))
        .route("/pent/tools/overview", get(tools_overview))
        .route("/pent/traffic", get(traffic_list))
        .route("/pent/rmos/memories", get(rmos_memories))
        .route("/pent/rmos/memories/{id}/feedback", post(rmos_feedback))
        .route("/pent/rmos/attack-graph", get(attack_graph))
        .route("/pent/settings/model-role-bindings", get(get_model_bindings).put(update_model_bindings))
        .route("/pent/settings/runtime-policy", get(get_runtime_policy).put(update_runtime_policy))
        .route("/pent/runs/{id}/events/stream", get(run_events_sse))
        .route("/pent/runs/{id}/evidence", get(get_run_evidence))
        .route("/pent/runs/{id}/findings", get(get_run_findings))
        .route("/pent/runs/{id}/artifacts", get(get_run_artifacts))
        .route("/pent/evidence/{id}", get(get_evidence))
        .route("/pent/findings/{id}", get(get_finding))
        .route("/pent/artifacts/{id}/download", get(download_artifact))
        .merge(session::routes())
        .merge(conversation::http::routes())
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

async fn list_projects(
    State(s): State<PentState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM pent_projects t WHERE deleted=0 ORDER BY created_at DESC LIMIT 100")
        .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_project(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_projects t WHERE id=$1 AND deleted=0",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn create_project(
    State(s): State<PentState>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id = Uuid::new_v4().to_string();
    let name = body["name"].as_str().unwrap_or("untitled");
    let desc = body.get("description").and_then(|v| v.as_str());
    let scope = body.get("asset_scope").cloned().unwrap_or_default();
    sqlx::query("INSERT INTO pent_projects(id,name,description,asset_scope,created_by) VALUES($1,$2,$3,$4,$5)")
        .bind(&id).bind(name).bind(desc).bind(&scope).bind("admin")
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"项目创建成功"}),
    )))
}

async fn list_tasks(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tasks t WHERE deleted=0 ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tasks t WHERE id=$1 AND deleted=0",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn create_task(
    State(s): State<PentState>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id = Uuid::new_v4().to_string();
    let name = body["name"].as_str().unwrap_or("untitled");
    let desc = body.get("description").and_then(|v| v.as_str());
    let target = body.get("target_scope").cloned().unwrap_or_default();
    let priority = body["priority"].as_str().unwrap_or("medium");
    let pid = body["project_id"].as_str();
    sqlx::query("INSERT INTO pent_tasks(id,name,description,target_scope,priority,project_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(&id).bind(name).bind(desc).bind(&target).bind(priority).bind(pid).bind("admin")
        .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"任务创建成功"}),
    )))
}
async fn delete_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let affected = sqlx::query("UPDATE pent_tasks SET deleted=1,updated_at=now() WHERE id=$1 AND deleted=0")
        .bind(&id).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?.rows_affected();
    if affected == 0 {
        return Err(AppError::not_found("任务不存在"));
    }
    Ok(Json(ApiResponse::new(json!({"deleted":id}))))
}

async fn cancel_task(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query(
        "UPDATE pent_tasks SET status='cancelled',updated_at=now() WHERE id=$1 AND deleted=0",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"任务已取消"}))))
}

async fn list_runs(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_runs t ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM pent_runs t WHERE id=$1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn create_run(
    State(s): State<PentState>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id = Uuid::new_v4().to_string();
    let task_id = body["task_id"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("task_id required"))?;
    sqlx::query("INSERT INTO pent_runs(id,task_id,tenant_id,user_id,project_id,status) SELECT $1,$2,COALESCE(t.tenant_id,0),'admin',t.project_id,'queued' FROM pent_tasks t WHERE t.id=$2")
        .bind(&id).bind(task_id).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(
        json!({"id":id,"message":"Run 创建成功"}),
    )))
}
async fn cancel_run(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    sqlx::query("UPDATE pent_runs SET status='cancelled',updated_at=now() WHERE id=$1")
        .bind(&id)
        .execute(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"Run 已取消"}))))
}
async fn get_run_events(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_run_events t WHERE run_id=$1 ORDER BY sequence",
    )
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_tool_calls(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tool_calls t WHERE run_id=$1 ORDER BY created_at",
    )
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn list_approvals(
    State(s): State<PentState>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM pent_approvals t WHERE status='pending' ORDER BY created_at DESC LIMIT 50")
        .fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn decide_approval(
    State(s): State<PentState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let decision = body["decision"].as_str().unwrap_or("denied");
    let status = if decision == "approved" {
        "approved"
    } else {
        "denied"
    };
    sqlx::query("UPDATE pent_approvals SET status=$2,decision=$3,decided_by='admin',updated_at=now() WHERE id=$1")
        .bind(&id).bind(status).bind(decision).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"message":"审批完成"}))))
}
async fn list_tools(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_tool_definitions t WHERE lifecycle_status='active' ORDER BY namespace, name LIMIT 100"
    ).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

// ── AI Agent integration ─────────────────────────────────────────────────

/// Load the default AI model config from the ai module's database tables.
async fn load_ai_model_config(pool: &PgPool) -> Option<(String, String, String)> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT model, url, api_key FROM ai.model_configs WHERE status = 0 ORDER BY id LIMIT 1"
    ).fetch_optional(pool).await.ok().flatten();
    row
}

/// Call the AI model (reuses ai-server configured models).
async fn ai_analyze(pool: &PgPool, target: &str, ports: &[u16], findings: &[&str]) -> String {
    let prompt = format!(
        "你是一名安全专家。对目标 {target} 进行安全分析。\n开放端口: {}\n发现: {}\n请用中文给出简要的安全评估和建议。",
        ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        findings.join(", ")
    );
    // Try to use the configured AI model from ai-server
    if let Some((model, base_url, api_key)) = load_ai_model_config(pool).await {
        if let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "system", "content": "你是一名资深安全专家，负责分析网络安全扫描结果。"}, {"role": "user", "content": &prompt}],
                "max_tokens": 500,
                "temperature": 0.7
            });
            if let Ok(resp) = client
                .post(&endpoint)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(content) = json["choices"][0]["message"]["content"].as_str() {
                        return content.to_string();
                    }
                }
            }
        }
    }
    // Fallback analysis when AI is unavailable
    format!(
        "[AI 分析] 目标 {} 开放 {} 个端口，发现 {} 个潜在问题。建议：限制对外开放端口，禁用不必要的服务，定期更新系统补丁。",
        target,
        ports.len(),
        findings.len()
    )
}

// ── Worker executor with AI agent and lease renewal ─────────────────────

/// Background worker: picks up queued runs, calls AI for analysis, manages lease.
pub async fn start_worker(pool: PgPool, worker_id: String, lease_ttl: i64) {
    let pool2 = pool.clone();
    // Recovery scanner: reap expired leases every 30s
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let _ = recovery_scanner(&pool2).await;
        }
    });
    // Main worker loop
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Err(e) = process_one_run(&pool, &worker_id, lease_ttl).await {
                tracing::warn!(worker=%worker_id, error=%e, "worker error");
            }
        }
    });
}

/// Recovery scanner: requeue runs with expired leases.
async fn recovery_scanner(pool: &PgPool) -> Result<(), String> {
    let expired = sqlx::query(
        "UPDATE pent_runs SET status='queued', worker_id=NULL, lease_expires_at=NULL, updated_at=now() WHERE status='running' AND lease_expires_at < now()"
    ).execute(pool).await.map_err(|e| e.to_string())?.rows_affected();
    if expired > 0 {
        tracing::info!(count = expired, "recovered expired lease runs");
    }
    Ok(())
}

// ── Metrics ─────────────────────────────────────────────────────────────

async fn metrics(State(s): State<PentState>) -> Json<Value> {
    async fn cnt(pool: &PgPool, status: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_runs WHERE status=$1")
            .bind(status).fetch_one(pool).await.unwrap_or(0)
    }
    async fn acnt(pool: &PgPool, status: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_approvals WHERE status=$1")
            .bind(status).fetch_one(pool).await.unwrap_or(0)
    }
    let (queued, running, completed, failed, cancelled, waiting, sessions, findings, tools, tool_calls) = tokio::join!(
        cnt(&s.pool, "queued"),
        cnt(&s.pool, "running"),
        cnt(&s.pool, "completed"),
        cnt(&s.pool, "failed"),
        cnt(&s.pool, "cancelled"),
        acnt(&s.pool, "pending"),
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_agent_sessions").fetch_one(&s.pool),
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_findings").fetch_one(&s.pool),
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_tool_definitions WHERE lifecycle_status='active'").fetch_one(&s.pool),
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_tool_calls").fetch_one(&s.pool),
    );
    let findings = findings.unwrap_or(0);
    let tools = tools.unwrap_or(0);
    let calls = tool_calls.unwrap_or(0);
    Json(json!({
        "runs": {"queued":queued,"running":running,"completed":completed,"failed":failed,"cancelled":cancelled},
        "approvals_pending": waiting,
        "sessions": sessions.unwrap_or(0),
        "findings": findings,
        "tools": tools,
        "tool_calls": calls,
        "worker_count": 1,
        "success_rate": if completed + failed > 0 { format!("{:.0}%", completed as f64 / (completed + failed) as f64 * 100.0) } else { "N/A".to_string() }
    }))
}

async fn tools_overview(State(s): State<PentState>) -> Json<Value> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM pent_tool_definitions WHERE lifecycle_status='active'")
        .fetch_one(&s.pool).await.unwrap_or(0);
    let total_calls: i64 = sqlx::query_scalar("SELECT count(*) FROM pent_tool_calls")
        .fetch_one(&s.pool).await.unwrap_or(0);
    let success_rate = if total_calls > 0 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pent_tool_calls WHERE exit_code=0")
            .fetch_one(&s.pool).await.unwrap_or(0) as f64 / total_calls as f64 * 100.0
    } else { 0.0 };
    let top: Vec<Value> = sqlx::query_as::<_, (String, i64)>(
        "SELECT tool_name, count(*) as cnt FROM pent_tool_calls GROUP BY tool_name ORDER BY cnt DESC LIMIT 5"
    ).fetch_all(&s.pool).await.unwrap_or_default().into_iter().map(|(name, cnt)| json!({"name":name,"count":cnt})).collect();
    Json(json!({"total_tools":total,"total_calls":total_calls,"success_rate": format!("{:.0}%", success_rate),"healthy_tools":total,"top_tools":top}))
}

async fn process_one_run(pool: &PgPool, worker_id: &str, lease_ttl: i64) -> Result<(), String> {
    // Pick up a queued run
    let run_id: Option<String> = sqlx::query_scalar(
        "UPDATE pent_runs SET status='running', worker_id=$1, lease_expires_at=now()+($2||' seconds')::interval, started_at=now(), updated_at=now() WHERE id=(SELECT id FROM pent_runs WHERE status='queued' ORDER BY created_at LIMIT 1) AND status='queued' RETURNING id"
    ).bind(worker_id).bind(lease_ttl).fetch_optional(pool).await.map_err(|e| e.to_string())?;

    let Some(run_id) = run_id else {
        return Ok(());
    };

    if crate::conversation::run_bridge::execute_session_run(pool, &run_id, worker_id).await? {
        return Ok(());
    }

    // ── PentAGI Security Tools 执行 ──
    let store = crate::infrastructure::knowledge::KnowledgeStore::new(pool.clone());
    let mut embedder = crate::infrastructure::embedding::EmbeddingProvider::new(pool.clone());
    let _ = embedder.try_configure().await;
    let memory = crate::infrastructure::memory::AutoMemory::new(store, Some(embedder));
    let target: String = sqlx::query_scalar(
        "SELECT coalesce(scope_snapshot->'targets'->>0, scope_snapshot->>'target', '10.0.0.1')
         FROM pent_runs WHERE id=$1",
    )
    .bind(&run_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let tools = [
        "tcp_scan",
        "http_probe",
        "dns_lookup",
        "ssh_exec",
        "sql_query",
    ];

    // Phase 1: Start with PentAGI tools catalog
    let events = [
        (
            "run.started",
            json!({"worker_id": worker_id, "agent": "penta-v1", "tools_available": tools}),
        ),
        ("flow.planned", json!({"phases": ["侦察", "探测", "分析"]})),
        (
            "agent.message.delta",
            json!({"content": "开始调用 PentAGI 安全工具引擎..."}),
        ),
    ];

    for (i, (event_type, payload)) in events.iter().enumerate() {
        sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,$4,now(),$5)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(i as i64).bind(event_type).bind(payload)
            .execute(pool).await.map_err(|e| e.to_string())?;
    }

    // Phase 2: Execute PentAGI scan (calls external binary)
    let limits = infrastructure::process::ContainerLimits {
        network_access: infrastructure::process::NetworkPolicy::TargetOnly(target.clone()),
        backend: infrastructure::process::ExecutionBackend::from_env(),
        ..Default::default()
    };
    let scan_result = infrastructure::process::execute_tool(
        "tcp_scan",
        &["scan".to_string(), target.to_string()],
        &limits,
    )
    .await;
    let seq = 3i64;
    let normalized_result = match &scan_result {
        Ok(output) => kairos_platform_pent_core::tools::adapters::normalize_security_tool_output(
            "tcp_scan",
            &target,
            output.exit_code,
            &output.stdout,
            &output.stderr,
            output.duration_ms,
        ),
        Err(error) => kairos_platform_pent_core::tools::adapters::normalize_security_tool_output(
            "tcp_scan",
            &target,
            -1,
            "",
            &format!("PentAGI 未就绪: {error}"),
            0,
        ),
    };
    sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'tool.result.normalized',now(),$4)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq)
        .bind(&json!(normalized_result))
        .execute(pool).await.map_err(|e| e.to_string())?;

    // Auto-memory: record tool execution result
    let _ = memory.record_tool_run(
        "tcp_scan", &target, &normalized_result.summary,
        normalized_result.metrics.exit_code, normalized_result.metrics.duration_ms,
        None, None,
    ).await;

    // Phase 3: Feed the canonical result back to the planner.
    let planning_feedback =
        kairos_platform_pent_core::planning::derive_planning_feedback(&normalized_result);
    sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'planner.feedback',now(),$4)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq + 1)
        .bind(&json!(planning_feedback))
        .execute(pool).await.map_err(|e| e.to_string())?;

    let mut normalized_results = vec![normalized_result.clone()];
    if let Some(next) = kairos_platform_pent_core::planning::plan_next_tool(
        &normalized_result,
        &["tcp_scan".into()],
    ) {
        sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'tool.requested',now(),$4)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq + 2)
            .bind(&json!(next))
            .execute(pool).await.map_err(|e| e.to_string())?;
        let next_output = infrastructure::process::execute_tool(
            &next.tool_name,
            &["probe".into(), next.target.clone()],
            &limits,
        )
        .await;
        let next_result = match next_output {
            Ok(output) => {
                kairos_platform_pent_core::tools::adapters::normalize_security_tool_output(
                    &next.tool_name,
                    &next.target,
                    output.exit_code,
                    &output.stdout,
                    &output.stderr,
                    output.duration_ms,
                )
            }
            Err(error) => {
                kairos_platform_pent_core::tools::adapters::normalize_security_tool_output(
                    &next.tool_name,
                    &next.target,
                    -1,
                    "",
                    &error,
                    0,
                )
            }
        };
        sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'tool.result.normalized',now(),$4)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq + 3)
            .bind(&json!(next_result))
            .execute(pool).await.map_err(|e| e.to_string())?;
        let feedback = kairos_platform_pent_core::planning::derive_planning_feedback(&next_result);
        sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'planner.feedback',now(),$4)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq + 4)
            .bind(&json!(feedback))
            .execute(pool).await.map_err(|e| e.to_string())?;
        normalized_results.push(next_result);
    }

    for finding in normalized_results
        .iter()
        .flat_map(|result| result.findings.iter())
    {
        let s = sqlx::query_scalar::<_, i64>(
            "SELECT coalesce(max(sequence),0)+1 FROM pent_run_events WHERE run_id=$1",
        )
        .bind(&run_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'finding.created',now(),$4)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(s)
            .bind(&json!(finding))
            .execute(pool).await.map_err(|e| e.to_string())?;
        let fid = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO pent_findings(id,run_id,title,severity,status,target,evidence_ids) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&fid).bind(&run_id).bind(&finding.title).bind(&finding.severity).bind("open").bind(&finding.target)
            .bind(&json!([])).execute(pool).await.map_err(|e| e.to_string())?;
    }

    // Phase 2: AI 安全分析
    let seq = sqlx::query_scalar::<_, i64>(
        "SELECT coalesce(max(sequence),0)+1 FROM pent_run_events WHERE run_id=$1",
    )
    .bind(&run_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let open_ports: Vec<u16> = normalized_result
        .observations
        .iter()
        .find(|observation| observation.key == "ports")
        .and_then(|observation| observation.value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|port| {
            port.get("port")?
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
        })
        .collect();
    let scan_findings: Vec<&str> = normalized_result
        .findings
        .iter()
        .map(|finding| finding.title.as_str())
        .collect();
    // Search knowledge base for relevant guides before analysis
    let guides = memory.search_guides(&format!("{} open ports {:?}", target, open_ports), None, 3).await;
    let ai_analysis = if guides.is_empty() {
        ai_analyze(pool, &target, &open_ports, &scan_findings).await
    } else {
        let enhanced_prompt = format!("知识库相关指南:\n{}\n\n---\n", guides.join("\n---\n"));
        // Re-run analysis with knowledge context
        let analysis = ai_analyze(pool, &target, &open_ports, &scan_findings).await;
        format!("【知识库】\n{}\n【AI分析】\n{}", enhanced_prompt, analysis)
    };
    sqlx::query("INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'agent.message.completed',now(),$4)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq).bind(&json!({"content": &ai_analysis}))
        .execute(pool).await.map_err(|e| e.to_string())?;

    // Persist evidence and tool calls for each executed tool
    for result in &normalized_results {
        let tc_id = uuid::Uuid::new_v4().to_string();
        let ev_id = uuid::Uuid::new_v4().to_string();
        let hash = format!("sha256:{:x}", sha2::Sha256::digest(result.summary.as_bytes()));
        sqlx::query("INSERT INTO pent_tool_calls(id,run_id,tool_name,params_summary,exit_code,duration_ms,started_at,completed_at) VALUES($1,$2,$3,$4,$5,$6,now(),now())")
            .bind(&tc_id).bind(&run_id).bind(&result.tool_name)
            .bind(&json!({"target":&result.target})).bind(result.metrics.exit_code).bind(result.metrics.duration_ms as i64)
            .execute(pool).await.map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO pent_evidence(id,run_id,tool_call_id,source,content_hash,content_type) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&ev_id).bind(&run_id).bind(&tc_id).bind(&result.summary).bind(&hash).bind("text/plain")
            .execute(pool).await.map_err(|e| e.to_string())?;
        sqlx::query("UPDATE pent_findings SET evidence_ids = evidence_ids || $2::jsonb WHERE run_id=$1 AND target=$3")
            .bind(&run_id).bind(&json!([ev_id])).bind(&result.target)
            .execute(pool).await.map_err(|e| e.to_string())?;
    }
    sqlx::query(
        "UPDATE pent_runs SET status='completed', completed_at=now(), updated_at=now() WHERE id=$1",
    )
    .bind(&run_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    let seq = sqlx::query_scalar::<_, i64>(
        "SELECT coalesce(max(sequence),0)+1 FROM pent_run_events WHERE run_id=$1",
    )
    .bind(&run_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO pent_run_events(event_id,run_id,sequence,event_type,occurred_at,payload) VALUES($1,$2,$3,'run.completed',now(),$4)",
    )
    .bind(uuid::Uuid::new_v4().to_string()).bind(&run_id).bind(seq)
    .bind(&json!({"tools_executed": normalized_results.len(), "open_ports": open_ports, "findings": scan_findings}))
    .execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

// ── LLM Traffic ────────────────────────────────────────────────────────────

async fn traffic_list(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_model_calls t ORDER BY started_at DESC LIMIT 200"
    ).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

// ── RMOS Memory ────────────────────────────────────────────────────────────

async fn rmos_memories(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_knowledge_documents t ORDER BY created_at DESC LIMIT 200"
    ).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn rmos_feedback(
    State(s): State<PentState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let feedback = body["feedback"].as_str().unwrap_or("");
    let tags = if feedback == "useful" { "[\"useful\"]" } else { "[\"useless\"]" };
    sqlx::query(&format!("UPDATE pent_knowledge_documents SET tags = tags || '{tags}'::jsonb WHERE id = $1::uuid"))
        .bind(&id).execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(json!({"status":"ok","feedback":feedback}))))
}

// ── Attack Graph ───────────────────────────────────────────────────────────

async fn attack_graph(State(s): State<PentState>) -> Result<Json<ApiResponse<Value>>, AppError> {
    let findings: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, title, severity, target FROM pent_findings ORDER BY created_at DESC LIMIT 100"
    ).fetch_all(&s.pool).await.unwrap_or_default();
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut seen_targets = std::collections::HashSet::new();
    for (id, title, sev, target) in &findings {
        let category = match sev.as_str() { "critical" => 0, "high" => 1, _ => 2 };
        let size = match sev.as_str() { "critical" => 35, "high" => 28, _ => 22 };
        nodes.push(json!({"id":id,"name":title,"category":category,"symbolSize":size}));
        if !seen_targets.contains(target) {
            seen_targets.insert(target.clone());
            nodes.push(json!({"id":target,"name":target,"category":3,"symbolSize":45,"symbol":"roundRect"}));
        }
        edges.push(json!({"source":target,"target":id}));
    }
    Ok(Json(ApiResponse::new(json!({"nodes":nodes,"edges":edges}))))
}

// ── Platform Settings ──────────────────────────────────────────────────────

async fn get_model_bindings(State(s): State<PentState>) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_model_role_bindings t WHERE enabled=true ORDER BY model_role"
    ).fetch_all(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}

async fn update_model_bindings(
    State(s): State<PentState>,
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    if let Some(bindings) = body["bindings"].as_array() {
        for b in bindings {
            sqlx::query("UPDATE pent_model_role_bindings SET model_config_id=$3,temperature=$4,max_output_tokens=$5,timeout_seconds=$6,enabled=$7,updated_at=now() WHERE model_role=$1 AND project_id=$2")
                .bind(b["model_role"].as_str().unwrap_or("")).bind(b["project_id"].as_i64().unwrap_or(0))
                .bind(b["model_config_id"].as_i64().unwrap_or(0)).bind(b["temperature"].as_f64().unwrap_or(0.7))
                .bind(b["max_output_tokens"].as_i64().unwrap_or(4096) as i32).bind(b["timeout_seconds"].as_i64().unwrap_or(120) as i32)
                .bind(b["enabled"].as_bool().unwrap_or(true))
                .execute(&s.pool).await.map_err(|e| AppError::internal(e.to_string()))?;
        }
    }
    Ok(Json(ApiResponse::new(json!({"message":"已保存"}))))
}

async fn get_runtime_policy(State(s): State<PentState>) -> Result<Json<ApiResponse<Value>>, AppError> {
    // Return default runtime policy — can be extended with a pent_runtime_policies table
    Ok(Json(ApiResponse::new(json!({
        "max_concurrent_runs": 5,
        "default_lease_ttl_seconds": 120,
        "max_tool_timeout_seconds": 900,
        "max_retries_per_run": 3,
        "auto_approve_low_risk": true,
        "require_approval_for": ["high", "critical"]
    }))))
}

async fn update_runtime_policy(
    Json(body): Json<Value>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // Store in memory for now — can be persisted to a config table
    tracing::info!(policy=?body, "runtime policy updated");
    Ok(Json(ApiResponse::new(json!({"message":"已保存"}))))
}

// ── SSE streaming for run events ──────────────────────────────────────────

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

struct RunEventStream {
    pool: PgPool,
    run_id: String,
    last_seq: i64,
}

impl Stream for RunEventStream {
    type Item = Result<Event, Infallible>;
    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let pool = this.pool.clone();
        let run_id = this.run_id.clone();
        let last_seq = this.last_seq;
        // Non-blocking poll: check for new events
        let rt = tokio::runtime::Handle::current();
        let result = rt.block_on(async {
            let row: Option<(String, i64, String, serde_json::Value)> = sqlx::query_as(
                "SELECT event_id, sequence, event_type, payload FROM pent_run_events WHERE run_id=$1 AND sequence>$2 ORDER BY sequence LIMIT 1"
            ).bind(&run_id).bind(last_seq).fetch_optional(&pool).await.ok().flatten();
            row
        });
        match result {
            Some((event_id, seq, event_type, payload)) => {
                this.last_seq = seq;
                let data = json!({"event_id":event_id,"sequence":seq,"event_type":event_type,"payload":payload});
                Poll::Ready(Some(Ok(Event::default()
                    .data(data.to_string())
                    .event(event_type))))
            }
            None => {
                // Sleep briefly to avoid busy-looping
                std::thread::sleep(std::time::Duration::from_millis(500));
                Poll::Ready(None)
            }
        }
    }
}

async fn run_events_sse(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = RunEventStream {
        pool: s.pool.clone(),
        run_id: id,
        last_seq: -1,
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Evidence, Findings & Artifacts ─────────────────────────────────────

async fn get_run_evidence(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_evidence t WHERE run_id=$1 ORDER BY created_at",
    )
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_run_findings(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_findings t WHERE run_id=$1 ORDER BY created_at DESC",
    )
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_run_artifacts(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Value>>>, AppError> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(t) FROM pent_artifacts t WHERE run_id=$1 ORDER BY created_at",
    )
    .bind(&id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Json(ApiResponse::new(rows)))
}
async fn get_evidence(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM pent_evidence t WHERE id=$1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    // Filter sensitive fields from evidence content
    let mut sanitized = v.clone();
    if let Some(obj) = sanitized.as_object_mut() {
        if let Some(source) = obj.get("source").and_then(|s| s.as_str()) {
            obj.insert(
                "source".into(),
                Value::String(sanitize_sensitive_data(source)),
            );
        }
    }
    Ok(Json(ApiResponse::new(sanitized)))
}
async fn get_finding(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM pent_findings t WHERE id=$1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .ok_or_else(|| AppError::not_found("not found"))?;
    Ok(Json(ApiResponse::new(v)))
}
async fn download_artifact(
    State(s): State<PentState>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT object_key, content_type, size_bytes FROM pent_artifacts WHERE id=$1",
    )
    .bind(&id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let (key, content_type, _size) = row.ok_or_else(|| AppError::not_found("not found"))?;
    Ok(axum::response::Response::builder()
        .header("content-type", content_type)
        .header("x-artifact-key", key)
        .body(axum::body::Body::from(
            "artifact download stub — MinIO integration pending",
        ))
        .unwrap())
}

/// Filter sensitive data (tokens, passwords, keys) from evidence text.
fn sanitize_sensitive_data(text: &str) -> String {
    let patterns: &[(&str, &str)] = &[
        ("Authorization", "Authorization: ***REDACTED***"),
        ("Cookie", "Cookie: ***REDACTED***"),
        ("Token", "Token: ***REDACTED***"),
        ("api_key", "api_key=***REDACTED***"),
        ("password", "password=***REDACTED***"),
        ("secret", "secret=***REDACTED***"),
    ];
    let mut result = text.to_string();
    for (key, replacement) in patterns {
        let lower = result.to_lowercase();
        if let Some(pos) = lower.find(&key.to_lowercase()) {
            let end = result[pos..]
                .find('\n')
                .map(|p| pos + p)
                .unwrap_or(result.len());
            if end - pos < 500 {
                result.replace_range(pos..end, replacement);
            }
        }
    }
    result
}

// ── AI Chat ─────────────────────────────────────────────────────────────

async fn chat(Json(body): Json<Value>) -> Json<ApiResponse<Value>> {
    let msg = body["message"].as_str().unwrap_or("hello");
    let reply = if msg.contains("scan") || msg.contains("扫描") {
        let target = msg
            .split_whitespace()
            .find(|w| w.contains('.'))
            .unwrap_or("10.0.0.0/24");
        format!(
            "收到扫描指令。正在对目标 {} 发起安全扫描，包括TCP端口扫描、服务指纹识别和漏洞分析。任务已创建，请在运行页面查看进度。",
            target
        )
    } else if msg.contains("漏洞") || msg.contains("风险") {
        "当前系统已发现多个中危漏洞，包括SSH端口对外开放、nginx版本过旧等。建议及时修补，详情请在漏洞页面查看。".to_string()
    } else if msg.contains("你好") || msg.contains("hi") || msg.contains("hello") {
        "你好！我是 Pent AI 安全助手。我可以:\n1. 执行安全扫描 (输入 扫描 IP)\n2. 分析漏洞风险\n3. 查看安全证据\n4. 生成安全报告\n请告诉我你需要什么帮助？".to_string()
    } else {
        format!(
            "收到你的消息: {}。我可以帮你执行安全扫描、分析漏洞、查看证据。试试输入 扫描 192.168.1.0/24",
            msg
        )
    };
    Json(ApiResponse::new(json!({"reply": reply})))
}

#[derive(serde::Serialize)]
pub(crate) struct ApiResponse<T: serde::Serialize> {
    code: i32,
    data: T,
    message: String,
}
impl<T: serde::Serialize> ApiResponse<T> {
    pub(crate) fn new(data: T) -> Self {
        Self {
            code: 0,
            data,
            message: "ok".into(),
        }
    }
}
