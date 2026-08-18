//! In-process MITM (man-in-the-middle) HTTP/HTTPS proxy.
//!
//! Intercepts outbound tool traffic, terminates TLS using a dynamically-issued
//! per-host certificate signed by a platform-owned CA, and forwards captured
//! exchanges to a [`TrafficSink`] for persistence (Postgres + object store).
//!
//! Embed in `pent-agent` (or the gateway) and point tools at it via the
//! existing `http_proxy`/`https_proxy` injection in `process.rs`.

pub mod ca;
pub mod proxy;
pub mod tls;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

/// One captured HTTP exchange, ready to persist.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrafficRecord {
    pub method: String,
    pub url: String,
    pub request_headers: serde_json::Value,
    pub request_body: Option<String>,
    pub response_status: Option<i32>,
    pub response_headers: serde_json::Value,
    pub response_body: Option<String>,
    pub duration_ms: Option<i64>,
    pub run_id: Option<String>,
}

/// Receives captured traffic. Implemented by `pent-server` to persist records.
#[async_trait]
pub trait TrafficSink: Send + Sync {
    async fn record(&self, record: TrafficRecord) -> anyhow::Result<()>;
}

/// Proxy configuration.
#[derive(Debug, Clone)]
pub struct MitmConfig {
    /// Listen address, e.g. `127.0.0.1:8085`.
    pub bind_addr: String,
    /// Directory where the CA key/cert are persisted.
    pub ca_dir: PathBuf,
    /// Max bytes of a request/response body kept for capture.
    pub body_limit_bytes: usize,
}

impl MitmConfig {
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("PENTA_MITM_BIND")
            .ok()
            .or_else(|| {
                // Fall back to the tool-injection proxy address (`http://host:port`).
                std::env::var("PENTA_TRAFFIC_PROXY").ok().map(|p| {
                    p.trim_start_matches("http://")
                        .trim_start_matches("https://")
                        .to_string()
                })
            })
            .unwrap_or_else(|| "127.0.0.1:8085".into());
        Self {
            bind_addr,
            ca_dir: ca::default_ca_dir(),
            body_limit_bytes: std::env::var("PENTA_MITM_BODY_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1024 * 1024),
        }
    }
}

/// A running MITM proxy handle.
#[derive(Clone)]
pub struct MitmProxy {
    ca: Arc<ca::CertificateAuthority>,
    config: MitmConfig,
}

impl MitmProxy {
    /// Load or generate the CA, and prepare the proxy.
    pub fn new(config: MitmConfig) -> Result<Self, String> {
        let ca = ca::CertificateAuthority::load_or_create(&config.ca_dir)?;
        Ok(Self { ca, config })
    }

    /// PEM encoding of the CA certificate, for injecting trust into tools.
    pub fn ca_cert_pem(&self) -> &str {
        self.ca.cert_pem()
    }

    /// Bind the listener and serve forever.
    pub async fn run(self, sink: Arc<dyn TrafficSink>) -> Result<(), String> {
        proxy::serve(self.config.clone(), self.ca.clone(), sink).await
    }

    /// Spawn the proxy on a background task.
    pub fn spawn(self, sink: Arc<dyn TrafficSink>) -> tokio::task::JoinHandle<Result<(), String>> {
        tokio::spawn(async move { self.run(sink).await })
    }
}
