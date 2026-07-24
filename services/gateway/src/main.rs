use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use kairos_platform_framework_common::{ApiResponse, ServiceConfig, health_route, init_tracing, serve};
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
    modules: [&'static str; 7],
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
    let toon_state = kairos_platform_toon_server::ToonState::new(database.clone(), tokens.clone());
    let media_state = kairos_platform_media_server::MediaState::new(database.clone(), tokens);
    let scan_state = kairos_platform_scan_server::ScanState::new(database.clone());
    let pent_state = kairos_platform_pent_server::PentState::new(database.clone());
    kairos_platform_pent_server::start_worker(database.clone(), "worker-gateway".into(), 120).await;
    system_state.bootstrap().await?;
    let database_auth = system_state.database_auth_state();

    let mut app = Router::new()
        .route("/", get(index))
        .route("/openapi.json", get(openapi::document))
        .merge(kairos_platform_system_server::routes(system_state))
        .merge(kairos_platform_infra_server::routes(infra_state))
        .merge(kairos_platform_ai_server::routes(ai_state))
        .merge(kairos_platform_toon_server::routes(toon_state))
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

async fn not_found() -> AppError {
    AppError::not_found("route not found")
}

async fn index() -> Json<ApiResponse<GatewayIndex>> {
    Json(ApiResponse::new(GatewayIndex {
        service: SERVICE_NAME,
        modules: ["system", "infra", "ai", "toon", "media", "scan", "pent"],
    }))
}
