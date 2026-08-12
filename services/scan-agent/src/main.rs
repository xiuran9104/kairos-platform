use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::{env, process::Command, time::Duration};

#[derive(Serialize)]
struct RegisterRequest<'a> {
    token: &'a str,
    agent_version: &'static str,
    hostname: &'a str,
    ip_address: Option<String>,
    capabilities: [&'static str; 2],
}

#[derive(Serialize)]
struct HeartbeatRequest<'a> {
    node_id: &'a str,
    token: &'a str,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Deserialize)]
struct Registration {
    node_id: String,
    heartbeat_interval_seconds: u64,
}

fn argument(name: &str) -> anyhow::Result<String> {
    let args: Vec<String> = env::args().collect();
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
        .with_context(|| format!("missing required argument {name}"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    if env::args().nth(1).as_deref() != Some("register") {
        bail!("usage: kairos-scan-agent register --server <url> --token <token> --name <hostname>");
    }
    Command::new("rustscan")
        .arg("--version")
        .output()
        .context("RustScan is required but was not found in PATH")?;

    let server = argument("--server")?.trim_end_matches('/').to_string();
    let token = argument("--token")?;
    let hostname = argument("--name")?;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{server}/asset-ops/scanner-agents/register"))
        .json(&RegisterRequest {
            token: &token,
            agent_version: env!("CARGO_PKG_VERSION"),
            hostname: &hostname,
            ip_address: None,
            capabilities: ["host_discovery", "port_scan"],
        })
        .send()
        .await?
        .error_for_status()?
        .json::<Envelope<Registration>>()
        .await?;
    tracing::info!(node_id=%response.data.node_id, "scanner agent registered");

    let interval = response.data.heartbeat_interval_seconds.max(10);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        client
            .post(format!("{server}/asset-ops/scanner-agents/heartbeat"))
            .json(&HeartbeatRequest {
                node_id: &response.data.node_id,
                token: &token,
            })
            .send()
            .await?
            .error_for_status()?;
        tracing::debug!(node_id=%response.data.node_id, "heartbeat sent");
    }
}
