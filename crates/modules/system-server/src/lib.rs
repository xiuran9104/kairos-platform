mod application;
mod audit;
mod bootstrap;
mod cache;
mod database_auth;
mod infrastructure;
mod management;
mod oauth2_token;
mod transport;

use kairos_platform_framework_database::PgPool;
use kairos_platform_framework_redis::RedisClient;
use kairos_platform_framework_security::{PasswordService, TokenService};

#[derive(Clone)]
pub struct SystemState {
    pool: PgPool,
    tokens: TokenService,
    passwords: PasswordService,
    cache: Option<RedisClient>,
}

impl SystemState {
    pub fn new(pool: PgPool, tokens: TokenService) -> Self {
        Self::with_cache(pool, tokens, None)
    }

    pub fn with_cache(pool: PgPool, tokens: TokenService, cache: Option<RedisClient>) -> Self {
        Self {
            pool,
            tokens,
            passwords: PasswordService::default(),
            cache,
        }
    }

    pub async fn bootstrap(&self) -> anyhow::Result<()> {
        bootstrap::initialize(self).await
    }

    pub fn database_auth_state(&self) -> DatabaseAuthState {
        DatabaseAuthState::new(self.clone())
    }
}

pub use database_auth::{DatabaseAuthState, authenticate_from_database};
pub use transport::routes;
