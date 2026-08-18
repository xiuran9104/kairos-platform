//! Pent Agent — standalone security automation microservice.
//!
//! Owns Agent runtime, MCP, tool/sandbox execution, evidence production and
//! the asynchronous run worker. HTTP business APIs remain in the gateway.

use axum::Router;
use kairos_platform_framework_common::{ServiceConfig, health_route, init_tracing, serve};
use kairos_platform_framework_web::{WebConfig, apply_web_layers};
use kairos_platform_pent_server::PentConfig;

const SERVICE_NAME: &str = "pent-agent";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing(SERVICE_NAME);

    kairos_platform_pent_server::infrastructure::process::verify_bubblewrap()
        .map_err(anyhow::Error::msg)?;
    kairos_platform_pent_server::infrastructure::traffic_capture::verify_runtime()
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

    let config = PentConfig::from_env();
    let nats_bus = kairos_platform_pent_server::transport::mq::NatsEventBus::connect(
        &std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into()),
    )
    .await;
    kairos_platform_pent_server::transport::mq::init_conversation_tool_bus(nats_bus.handle())
        .map_err(anyhow::Error::msg)?;
    let worker_id = format!(
        "worker-{}-{}-{}",
        SERVICE_NAME,
        config.worker_id,
        uuid::Uuid::new_v4()
    );
    let _heartbeat = kairos_platform_pent_server::transport::mq::spawn_worker_heartbeat(
        nats_bus.handle(),
        worker_id,
    );
    let agent_config = nats_bus
        .handle()
        .request_agent_config()
        .await
        .map_err(anyhow::Error::msg)?;
    let recipe_count = kairos_platform_pent_server::infrastructure::recipe::load_runtime_snapshot(
        agent_config.recipes,
    );
    tracing::info!(recipe_count, "tool recipes synchronized from Gateway");
    let _ = kairos_platform_pent_server::infrastructure::execution::init_global_executor(
        std::sync::Arc::new(
            kairos_platform_pent_server::infrastructure::execution::ElasticExecutor::from_env(),
        ),
    );
    if let Err(error) =
        kairos_platform_pent_server::transport::mq::spawn_conversation_tool_responder(&nats_bus)
            .await
    {
        tracing::warn!(%error, "conversation tool responder unavailable");
    }
    if let Err(error) =
        kairos_platform_pent_server::transport::mq::spawn_terminal_responder(&nats_bus).await
    {
        tracing::warn!(%error, "governed terminal responder unavailable");
    }

    // Start the in-process MITM traffic-capture proxy when enabled.
    let _mitm_proxy = kairos_platform_pent_server::infrastructure::mitm_sink::spawn_mitm_proxy_nats(
        nats_bus.handle(),
    );

    // The agent exposes health only. All authenticated business APIs, SSE and
    // task submission remain gateway responsibilities.
    let app = Router::new().merge(health_route(SERVICE_NAME));
    let app = apply_web_layers(app, WebConfig::from_env());

    serve(ServiceConfig::from_env(SERVICE_NAME, 8081), app).await
}
