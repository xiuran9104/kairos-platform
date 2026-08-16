//! Pent Agent — standalone security automation microservice.
//!
//! Owns Agent runtime, MCP, tool/sandbox execution, evidence production and
//! the asynchronous run worker. HTTP business APIs remain in the gateway.

use axum::Router;
use kairos_platform_framework_common::{ServiceConfig, health_route, init_tracing, serve};
use kairos_platform_framework_database::{DatabaseConfig, connect, ensure_initialized};
use kairos_platform_framework_web::{WebConfig, apply_web_layers};
use kairos_platform_pent_server::PentConfig;

const SERVICE_NAME: &str = "pent-agent";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing(SERVICE_NAME);

    kairos_platform_pent_server::infrastructure::process::verify_bubblewrap()
        .map_err(anyhow::Error::msg)?;

    match kairos_platform_pent_server::infrastructure::tool_lifecycle::sync_tool_directory() {
        Ok(report) => tracing::info!(
            linked = report.linked.len(),
            missing = ?report.missing,
            errors = ?report.errors,
            "controlled tool directory synced"
        ),
        Err(error) => tracing::warn!(%error, "controlled tool directory sync skipped"),
    }

    let database = connect(&DatabaseConfig::from_env()?).await?;
    ensure_initialized(&database).await?;
    let config = PentConfig::from_env();
    let mcp_registry = kairos_platform_pent_tooling::mcp::registry::shared().await;
    let worker_id = format!(
        "worker-{}-{}-{}",
        SERVICE_NAME,
        config.worker_id,
        uuid::Uuid::new_v4()
    );
    kairos_platform_pent_server::spawn_mcp_reconciler(
        database.clone(),
        mcp_registry,
        worker_id.clone(),
    );
    match kairos_platform_pent_server::infrastructure::recipe::load_from_db(&database).await {
        Ok(count) => tracing::info!(count, "tool recipes loaded from pent_tool_definitions"),
        Err(error) => {
            tracing::warn!(%error, "tool recipe load failed; falling back to hardcoded registry")
        }
    }
    let _ = kairos_platform_pent_server::infrastructure::execution::init_global_executor(
        std::sync::Arc::new(
            kairos_platform_pent_server::infrastructure::execution::ElasticExecutor::from_env(),
        ),
    );

    // Start the in-process MITM traffic-capture proxy when enabled.
    let _mitm_proxy =
        kairos_platform_pent_server::infrastructure::mitm_sink::spawn_mitm_proxy(database.clone());

    // Start the worker for async task processing
    kairos_platform_pent_server::start_worker(
        database.clone(),
        worker_id,
        config.lease_ttl_seconds,
    )
    .await;

    // The agent exposes health only. All authenticated business APIs, SSE and
    // task submission remain gateway responsibilities.
    let app = Router::new().merge(health_route(SERVICE_NAME));
    let app = apply_web_layers(app, WebConfig::from_env());

    serve(ServiceConfig::from_env(SERVICE_NAME, 8081), app).await
}
