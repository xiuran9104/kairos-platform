//! MITM traffic sink — persists captured exchanges into `pent_http_traffic`.
//!
//! Runs the in-process MITM proxy when `PENTA_TRAFFIC_CAPTURE=true` and
//! forwards every captured HTTP exchange to the sink. Metadata and (bounded)
//! bodies are stored directly in Postgres; the body bound is controlled by
//! `PENTA_MITM_BODY_LIMIT`.

use std::sync::Arc;

use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_mitm_proxy::{MitmConfig, MitmProxy, TrafficRecord, TrafficSink};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

const DEFAULT_RUN_ID: &str = "mitm-proxy";

pub struct MitmTrafficSink {
    pool: PgPool,
}

impl MitmTrafficSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TrafficSink for MitmTrafficSink {
    async fn record(&self, record: TrafficRecord) -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let run_id = record
            .run_id
            .clone()
            .unwrap_or_else(|| DEFAULT_RUN_ID.to_string());
        let response_sha256 = record
            .response_body
            .as_ref()
            .map(|body| format!("{:x}", Sha256::digest(body.as_bytes())));

        sqlx::query(
            "INSERT INTO pent_http_traffic(
                 id, run_id, tool_call_id, tool_name, method, url,
                 request_headers, request_body, response_status, response_headers,
                 response_body, response_sha256, duration_ms, replay_of, created_at
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now())",
        )
        .bind(&id)
        .bind(&run_id)
        .bind(Option::<String>::None)
        .bind("mitm_proxy")
        .bind(&record.method)
        .bind(&record.url)
        .bind(&record.request_headers)
        .bind(&record.request_body)
        .bind(record.response_status)
        .bind(&record.response_headers)
        .bind(&record.response_body)
        .bind(response_sha256)
        .bind(record.duration_ms)
        .bind(Option::<String>::None)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Start the MITM proxy in the background when traffic capture is enabled.
///
/// Returns `None` when `PENTA_TRAFFIC_CAPTURE` is not set to `true`, so
/// callers can simply ignore the result. The CA is persisted under
/// `PENTA_MITM_CA_DIR` (default `storage/mitm`), which the tool runner reads
/// back to inject trust into tool processes.
pub fn spawn_mitm_proxy(pool: PgPool) -> Option<tokio::task::JoinHandle<Result<(), String>>> {
    let enabled = std::env::var("PENTA_TRAFFIC_CAPTURE")
        .map(|v| v == "true")
        .unwrap_or(false);
    if !enabled {
        return None;
    }
    let config = MitmConfig::from_env();
    let proxy = match MitmProxy::new(config.clone()) {
        Ok(proxy) => proxy,
        Err(error) => {
            warn!(%error, "mitm proxy init failed; traffic capture disabled");
            return None;
        }
    };
    let sink = Arc::new(MitmTrafficSink::new(pool));
    info!(addr = %config.bind_addr, "mitm traffic capture enabled");
    Some(proxy.spawn(sink))
}
