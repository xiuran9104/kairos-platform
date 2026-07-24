//! Pent Agent — standalone security automation microservice.
//!
//! Provides Agent runtime, Flow orchestration, tool execution,
//! evidence collection, and run-time permission approval.

use kairos_platform_framework_common::{ServiceConfig, init_tracing, serve};
use kairos_platform_framework_database::{DatabaseConfig, connect, ensure_initialized};
use kairos_platform_framework_security::{SecurityConfig, TokenService};
use kairos_platform_framework_web::{WebConfig, apply_web_layers};
use kairos_platform_pent_server::{PentConfig, PentState, routes};

const SERVICE_NAME: &str = "pent-agent";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing(SERVICE_NAME);

    let database = connect(&DatabaseConfig::from_env()?).await?;
    ensure_initialized(&database).await?;
    let config = PentConfig::from_env();
    let tokens = TokenService::new(SecurityConfig::from_env()?);
    let state = PentState::new(database.clone());

    // Start the worker for async task processing
    kairos_platform_pent_server::start_worker(
        database.clone(),
        format!("worker-{}-{}", SERVICE_NAME, config.worker_id),
        60,
    )
    .await;

    let app = apply_web_layers(routes(state), WebConfig::from_env());

    serve(ServiceConfig::from_env(SERVICE_NAME, 8081), app).await
}
