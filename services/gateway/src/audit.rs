use std::time::Instant;

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use kairos_platform_framework_database::PgPool;

#[derive(Clone)]
pub struct AuditState {
    pool: PgPool,
}

impl AuditState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

pub async fn record(
    State(state): State<AuditState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let started_at = chrono::Utc::now().naive_utc();
    let timer = Instant::now();
    let method = request.method().to_string();
    let path = request
        .uri()
        .path_and_query()
        .map(ToString::to_string)
        .unwrap_or_else(|| request.uri().path().to_string());
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .chars()
        .take(500)
        .collect::<String>();
    let request_trace_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let response = next.run(request).await;
    let status = response.status().as_u16() as i32;
    let trace_id = request_trace_id
        .or_else(|| {
            response
                .headers()
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let duration = timer.elapsed().as_millis().min(i32::MAX as u128) as i32;
    let ended_at = chrono::Utc::now().naive_utc();
    let module = path
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("gateway")
        .to_string();
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(
            "INSERT INTO infra_api_access_log(
                trace_id, application_name, request_method, request_url,
                user_agent, operate_module, begin_time, end_time, duration,
                result_code, result_msg
             ) VALUES($1,'kairos-platform-gateway',$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(trace_id)
        .bind(method)
        .bind(path)
        .bind(user_agent)
        .bind(module)
        .bind(started_at)
        .bind(ended_at)
        .bind(duration)
        .bind(status)
        .bind(if status >= 400 { "failed" } else { "ok" })
        .execute(&pool)
        .await;
    });
    response
}
