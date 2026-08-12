//! Pent Agent server configuration.

use std::env;

#[derive(Debug, Clone)]
pub struct PentConfig {
    pub worker_id: String,
    pub lease_ttl_seconds: i64,
    pub max_retries: u32,
}

impl PentConfig {
    pub fn from_env() -> Self {
        Self {
            worker_id: env::var("PENTA_WORKER_ID").unwrap_or_else(|_| "worker-01".into()),
            lease_ttl_seconds: env::var("PENTA_LEASE_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(120),
            max_retries: env::var("PENTA_MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
        }
    }
}
