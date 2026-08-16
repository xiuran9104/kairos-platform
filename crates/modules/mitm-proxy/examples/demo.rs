//! Standalone demo of the MITM proxy with a stdout-printing sink.
//!
//! Run with:
//!   cargo run -p kairos-platform-mitm-proxy --example demo
//!
//! Then point a tool at it (default `127.0.0.1:8085`):
//!   curl -x http://127.0.0.1:8085 http://example.com/
//!   curl -x http://127.0.0.1:8085 https://example.com/ --cacert storage/mitm/ca-cert.pem

use std::sync::Arc;

use async_trait::async_trait;
use kairos_platform_mitm_proxy::{MitmConfig, MitmProxy, TrafficRecord, TrafficSink};

struct PrintSink;

#[async_trait]
impl TrafficSink for PrintSink {
    async fn record(&self, record: TrafficRecord) -> anyhow::Result<()> {
        let status = record.response_status.unwrap_or(0);
        println!("=== {} {} -> {} ===", record.method, record.url, status);
        if let Some(body) = &record.response_body {
            let preview: String = body.chars().take(120).collect();
            println!("    body: {preview}");
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let config = MitmConfig::from_env();
    let proxy = MitmProxy::new(config.clone())?;
    println!("CA cert: {}", config.ca_dir.join("ca-cert.pem").display());
    println!("listening on {}", config.bind_addr);
    proxy.run(Arc::new(PrintSink)).await
}
