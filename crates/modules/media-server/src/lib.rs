mod assets;
mod shared;

use axum::{Json, Router, middleware::from_fn_with_state, routing::get};
use kairos_platform_framework_common::ApiResponse;
use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_security::{TokenService, authenticate};
use kairos_platform_media_api::MediaCapability;

#[derive(Clone)]
pub struct MediaState {
    pool: PgPool,
    tokens: TokenService,
}

impl MediaState {
    pub fn new(pool: PgPool, tokens: TokenService) -> Self {
        Self { pool, tokens }
    }
}

pub fn routes(state: MediaState) -> Router {
    let protected = Router::new()
        .route("/media/assets", get(assets::list).post(assets::create))
        .route(
            "/media/assets/{id}",
            get(assets::get).put(assets::update).delete(assets::delete),
        )
        .route_layer(from_fn_with_state(state.tokens.clone(), authenticate));

    Router::new()
        .route("/media/capabilities", get(capabilities))
        .merge(protected)
        .with_state(state)
}

async fn capabilities() -> Json<ApiResponse<MediaCapability>> {
    Json(ApiResponse::new(MediaCapability::default()))
}
