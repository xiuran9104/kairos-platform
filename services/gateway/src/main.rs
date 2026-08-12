use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use kairos_platform_framework_common::{
    ApiResponse, ServiceConfig, health_route, init_tracing, serve,
};
use kairos_platform_framework_database::{DatabaseConfig, connect, ensure_initialized};
use kairos_platform_framework_redis::{RateLimitConfig, RateLimitState, RedisClient, RedisConfig};
use kairos_platform_framework_security::{SecurityConfig, TokenService};
use kairos_platform_framework_web::{AppError, WebConfig, apply_web_layers};
use serde::Serialize;
use tracing::warn;

mod audit;
mod openapi;

const SERVICE_NAME: &str = "gateway";

#[derive(Debug, Serialize)]
struct GatewayIndex {
    service: &'static str,
    modules: [&'static str; 6],
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing(SERVICE_NAME);

    let database = connect(&DatabaseConfig::from_env()?).await?;
    ensure_initialized(&database).await?;
    let redis = connect_redis().await;
    let tokens = TokenService::new(SecurityConfig::from_env()?);
    let system_state = kairos_platform_system_server::SystemState::with_cache(
        database.clone(),
        tokens.clone(),
        redis.clone(),
    );
    let infra_state = kairos_platform_infra_server::InfraState::new(database.clone());
    let ai_state = kairos_platform_ai_server::AiState::new(database.clone(), tokens.clone());
    let media_state = kairos_platform_media_server::MediaState::new(database.clone(), tokens);
    let scan_state = kairos_platform_scan_server::ScanState::new(database.clone());
    let nats_bus = kairos_platform_pent_server::transport::mq::NatsEventBus::connect(
        &std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into()),
    )
    .await;
    let mcp_registry = kairos_platform_pent_server::tooling::mcp::registry::shared().await;
    load_db_mcp_servers(&database, &mcp_registry).await;
    let pent_state = kairos_platform_pent_server::PentState::new(database.clone())
        .with_nats(nats_bus.handle())
        .with_mcp(mcp_registry);
    match kairos_platform_pent_server::infrastructure::recipe::load_from_db(&database).await {
        Ok(count) => tracing::info!(count, "tool recipes loaded from pent_tool_definitions"),
        Err(error) => warn!(%error, "tool recipe load failed; falling back to hardcoded registry"),
    }
    let _ = kairos_platform_pent_server::infrastructure::execution::init_global_executor(
        std::sync::Arc::new(
            kairos_platform_pent_server::infrastructure::execution::ElasticExecutor::from_env(),
        ),
    );
    kairos_platform_pent_server::start_worker(database.clone(), "worker-gateway".into(), 120).await;
    system_state.bootstrap().await?;
    // Recover orphaned conversation turns left from previous shutdown
    recover_orphaned_turns(&database).await;
    let database_auth = system_state.database_auth_state();

    let mut app = Router::new()
        .route("/", get(index))
        .route("/openapi.json", get(openapi::document))
        .merge(kairos_platform_system_server::routes(system_state))
        .merge(kairos_platform_infra_server::routes(infra_state))
        .merge(kairos_platform_ai_server::routes(ai_state))
        .merge(kairos_platform_media_server::routes(media_state))
        .merge(kairos_platform_scan_server::routes(scan_state))
        .merge(kairos_platform_pent_server::routes(pent_state))
        .merge(health_route(SERVICE_NAME))
        .fallback(not_found)
        .layer(from_fn_with_state(
            audit::AuditState::new(database),
            audit::record,
        ))
        .layer(from_fn_with_state(
            database_auth,
            kairos_platform_system_server::authenticate_from_database,
        ));

    if let Some(redis) = redis {
        app = app.layer(from_fn_with_state(
            RateLimitState::new(redis, RateLimitConfig::from_env()),
            kairos_platform_framework_redis::rate_limit,
        ));
    }

    let app = apply_web_layers(app, WebConfig::from_env());

    serve(ServiceConfig::from_env(SERVICE_NAME, 8080), app).await
}

async fn connect_redis() -> Option<RedisClient> {
    let config = RedisConfig::from_env()?;
    match RedisClient::connect(&config).await {
        Ok(client) => Some(client),
        Err(error) => {
            warn!(%error, "redis is configured but unavailable; cache and rate limit disabled");
            None
        }
    }
}

/// Start MCP servers that were configured through the capability center UI
/// and persisted in `pent_mcp_servers` (stdio transport only).
async fn load_db_mcp_servers(
    pool: &kairos_platform_framework_database::PgPool,
    registry: &kairos_platform_pent_server::tooling::mcp::registry::McpRegistry,
) {
    use kairos_platform_pent_server::tooling::mcp::protocol::McpServerConfig;
    let rows = sqlx::query_as::<_, (String, String, serde_json::Value, serde_json::Value)>(
        "SELECT name, command, args, env FROM pent_mcp_servers WHERE enabled AND transport='stdio'",
    )
    .fetch_all(pool)
    .await;
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            warn!(%error, "failed to load configured MCP servers");
            return;
        }
    };
    for (name, command, args, env) in rows {
        let config = McpServerConfig {
            id: name,
            command,
            args: serde_json::from_value(args).unwrap_or_default(),
            env: serde_json::from_value(env).unwrap_or_default(),
            auto_start: true,
        };
        if let Err(error) = registry.add_server(config).await {
            warn!(%error, "failed to start configured MCP server");
        }
    }
}

/// Recover any conversation turns orphaned by a previous shutdown.
async fn recover_orphaned_turns(pool: &kairos_platform_framework_database::PgPool) {
    let result = sqlx::query(
        "UPDATE pent_conversation_turns SET status='failed', error_message='网关重启自动恢复', retryable=true WHERE status IN ('thinking','queued') AND created_at < now() - interval '30 seconds'"
    )
    .execute(pool)
    .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(
                count = r.rows_affected(),
                "recovered orphaned conversation turns"
            );
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "failed to recover orphaned turns"),
    }
    let _ = sqlx::query(
        "UPDATE pent_agent_sessions SET status='ready' WHERE status='running' AND NOT EXISTS (SELECT 1 FROM pent_conversation_turns WHERE pent_conversation_turns.session_id = pent_agent_sessions.id AND status IN ('thinking','queued'))"
    )
    .execute(pool)
    .await;
}

async fn not_found() -> AppError {
    AppError::not_found("route not found")
}

async fn index() -> Json<ApiResponse<GatewayIndex>> {
    Json(ApiResponse::new(GatewayIndex {
        service: SERVICE_NAME,
        modules: ["system", "infra", "ai", "media", "scan", "pent"],
    }))
}
