//! Pent Agent — standalone security automation microservice.
//!
//! Provides Agent runtime, Flow orchestration, tool execution,
//! evidence collection, and run-time permission approval.

use axum::{
    extract::{Request, State},
    middleware::{Next, from_fn_with_state},
    response::Response,
};
use kairos_platform_framework_common::{ServiceConfig, init_tracing, serve};
use kairos_platform_framework_database::{DatabaseConfig, connect, ensure_initialized};
use kairos_platform_framework_security::{
    SecurityConfig, SecurityError, TokenService, authenticate,
};
use kairos_platform_framework_web::{WebConfig, apply_web_layers};
use kairos_platform_pent_server::{PentConfig, PentState, routes};

const SERVICE_NAME: &str = "pent-agent";

async fn authenticate_pent(
    State(tokens): State<TokenService>,
    request: Request,
    next: Next,
) -> Result<Response, SecurityError> {
    if request.uri().path() == "/pent/health" {
        return Ok(next.run(request).await);
    }
    authenticate(State(tokens), request, next).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing(SERVICE_NAME);

    let database = connect(&DatabaseConfig::from_env()?).await?;
    ensure_initialized(&database).await?;
    let config = PentConfig::from_env();
    let tokens = TokenService::new(SecurityConfig::from_env()?);
    let state = PentState::new(database.clone());
    match kairos_platform_pent_server::infrastructure::recipe::load_from_db(&database).await {
        Ok(count) => tracing::info!(count, "tool recipes loaded from pent_tool_definitions"),
        Err(error) => tracing::warn!(%error, "tool recipe load failed; falling back to hardcoded registry"),
    }
    let _ = kairos_platform_pent_server::infrastructure::execution::init_global_executor(
        std::sync::Arc::new(
            kairos_platform_pent_server::infrastructure::execution::ElasticExecutor::from_env(),
        ),
    );

    // Start the in-process MITM traffic-capture proxy when enabled.
    let _mitm_proxy = kairos_platform_pent_server::infrastructure::mitm_sink::spawn_mitm_proxy(
        database.clone(),
    );

    // Start the worker for async task processing
    kairos_platform_pent_server::start_worker(
        database.clone(),
        format!("worker-{}-{}", SERVICE_NAME, config.worker_id),
        60,
    )
    .await;

    let app = routes(state).layer(from_fn_with_state(tokens, authenticate_pent));
    let app = apply_web_layers(app, WebConfig::from_env());

    serve(ServiceConfig::from_env(SERVICE_NAME, 8081), app).await
}
