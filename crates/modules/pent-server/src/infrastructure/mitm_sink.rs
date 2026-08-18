//! MITM traffic sink — persists captured exchanges into `pent_http_traffic`.
//!
//! Runs the in-process MITM proxy unless `PENTA_TRAFFIC_CAPTURE=false` and
//! forwards every Run-attributed HTTP exchange to the sink. Metadata and
//! (bounded) bodies are stored directly in Postgres; the body bound is
//! controlled by `PENTA_MITM_BODY_LIMIT`.

use std::sync::Arc;

use async_trait::async_trait;
use kairos_platform_framework_database::PgPool;
use kairos_platform_mitm_proxy::{MitmConfig, MitmProxy, TrafficRecord, TrafficSink};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

const DEFAULT_INLINE_BODY_LIMIT: usize = 256 * 1024;

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
        let Some(run_id) = record.run_id.clone() else {
            tracing::debug!(url = %record.url, "skipping unattributed MITM traffic");
            return Ok(());
        };
        let id = uuid::Uuid::new_v4().to_string();
        let request_sha256 = record
            .request_body
            .as_ref()
            .map(|body| format!("{:x}", Sha256::digest(body.as_bytes())));
        let response_sha256 = record
            .response_body
            .as_ref()
            .map(|body| format!("{:x}", Sha256::digest(body.as_bytes())));
        let inline_limit = std::env::var("PENTA_MITM_INLINE_BODY_LIMIT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_INLINE_BODY_LIMIT)
            .clamp(1024, 16 * 1024 * 1024);
        let store = crate::infrastructure::object_store::ObjectStore::from_env();
        let (request_body, request_body_object_key) = persist_large_body(
            &store,
            &run_id,
            &id,
            "request",
            record.request_body,
            inline_limit,
        )
        .await;
        let (response_body, response_body_object_key) = persist_large_body(
            &store,
            &run_id,
            &id,
            "response",
            record.response_body,
            inline_limit,
        )
        .await;

        sqlx::query(
            "INSERT INTO pent_http_traffic(
                 id, run_id, tool_call_id, tool_name, method, url,
                 request_headers, request_body, response_status, response_headers,
                 response_body, response_sha256, duration_ms, replay_of, created_at,
                 request_sha256,request_body_object_key,response_body_object_key
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now(),$15,$16,$17)",
        )
        .bind(&id)
        .bind(&run_id)
        .bind(Option::<String>::None)
        .bind("mitm_proxy")
        .bind(&record.method)
        .bind(&record.url)
        .bind(&record.request_headers)
        .bind(&request_body)
        .bind(record.response_status)
        .bind(&record.response_headers)
        .bind(&response_body)
        .bind(response_sha256)
        .bind(record.duration_ms)
        .bind(Option::<String>::None)
        .bind(request_sha256)
        .bind(request_body_object_key)
        .bind(response_body_object_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

async fn persist_large_body(
    store: &crate::infrastructure::object_store::ObjectStore,
    run_id: &str,
    traffic_id: &str,
    direction: &str,
    body: Option<String>,
    inline_limit: usize,
) -> (Option<String>, Option<String>) {
    let Some(body) = body else {
        return (None, None);
    };
    if body.len() <= inline_limit {
        return (Some(body), None);
    }
    let object_key = format!("runs/{run_id}/http-traffic/{traffic_id}-{direction}.body");
    match store
        .put_object(
            &object_key,
            body.as_bytes().to_vec(),
            "application/octet-stream",
        )
        .await
    {
        Ok(()) => (None, Some(object_key)),
        Err(error) => {
            warn!(%error, %run_id, %traffic_id, %direction, "MITM body MinIO spill failed; storing bounded preview");
            let mut end = inline_limit.min(body.len());
            while !body.is_char_boundary(end) {
                end -= 1;
            }
            let preview = format!(
                "{}\n[MITM body truncated after MinIO spill failure]",
                &body[..end]
            );
            (Some(preview), None)
        }
    }
}

/// Whether process-level traffic capture is enabled. Capture is on by default
/// and can be explicitly disabled for constrained deployments.
pub(crate) fn traffic_capture_enabled() -> bool {
    let configured = std::env::var("PENTA_TRAFFIC_CAPTURE").ok();
    traffic_capture_enabled_from(configured.as_deref())
}

fn traffic_capture_enabled_from(configured: Option<&str>) -> bool {
    !configured.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}

/// Start the MITM proxy in the background when traffic capture is enabled.
///
/// Returns `None` when `PENTA_TRAFFIC_CAPTURE` is explicitly disabled, so
/// callers can simply ignore the result. The CA is persisted under
/// `PENTA_MITM_CA_DIR` (default `storage/mitm`), which the tool runner reads
/// back to inject trust into tool processes.
pub fn spawn_mitm_proxy(pool: PgPool) -> Option<tokio::task::JoinHandle<Result<(), String>>> {
    if !traffic_capture_enabled() {
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

#[cfg(test)]
mod tests {
    use super::traffic_capture_enabled_from;

    #[test]
    fn traffic_capture_defaults_to_enabled() {
        assert!(traffic_capture_enabled_from(None));
        assert!(traffic_capture_enabled_from(Some("true")));
        assert!(!traffic_capture_enabled_from(Some("false")));
        assert!(!traffic_capture_enabled_from(Some("0")));
    }
}
