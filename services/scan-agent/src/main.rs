use anyhow::{Context, bail};
use kairos_platform_scan_api::{
    ScannerHeartbeatRequest, ScannerPortResult, ScannerTargetResult, ScannerTaskClaim,
    ScannerTaskClaimRequest, ScannerTaskCompleteRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{env, net::Ipv4Addr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{RwLock, Semaphore},
    task::JoinSet,
    time::timeout,
};
use x509_parser::{extensions::GeneralName, parse_x509_certificate};

const MAX_TARGETS_PER_TASK: usize = 4096;
const SCAN_CONCURRENCY: usize = 128;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_BANNER_BYTES: usize = 512;
const MAX_HTTP_BODY_BYTES: usize = 16 * 1024;

#[derive(Serialize)]
struct RegisterRequest<'a> {
    token: &'a str,
    agent_version: &'static str,
    hostname: &'a str,
    ip_address: Option<String>,
    capabilities: [&'static str; 4],
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

fn expand_targets(raw: &str) -> anyhow::Result<Vec<String>> {
    let mut targets = Vec::new();
    for value in raw.split(&[',', ' ', '\n', ';']) {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some((address, prefix)) = value.split_once('/') {
            let address: Ipv4Addr = address
                .parse()
                .with_context(|| format!("unsupported CIDR target {value}"))?;
            let prefix: u32 = prefix
                .parse()
                .with_context(|| format!("invalid CIDR prefix {value}"))?;
            if prefix > 32 {
                bail!("invalid CIDR prefix {value}");
            }
            let host_count = 1_u64 << (32 - prefix);
            let usable_count = if prefix <= 30 {
                host_count.saturating_sub(2)
            } else {
                host_count
            };
            if usable_count as usize + targets.len() > MAX_TARGETS_PER_TASK {
                bail!("task expands beyond {MAX_TARGETS_PER_TASK} targets");
            }
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            let network = u32::from(address) & mask;
            let start = if prefix <= 30 { 1 } else { 0 };
            let end = if prefix <= 30 {
                host_count - 1
            } else {
                host_count
            };
            for offset in start..end {
                targets.push(Ipv4Addr::from(network + offset as u32).to_string());
            }
        } else {
            targets.push(value.to_string());
        }
        if targets.len() > MAX_TARGETS_PER_TASK {
            bail!("task expands beyond {MAX_TARGETS_PER_TASK} targets");
        }
    }
    if targets.is_empty() {
        bail!("task has no targets");
    }
    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn known_service(port: u16) -> Option<String> {
    match port {
        21 => Some("FTP"),
        22 => Some("SSH"),
        25 | 465 | 587 => Some("SMTP"),
        53 => Some("DNS"),
        80 | 8000 | 8008 | 8080 | 8081 | 8888 => Some("HTTP"),
        110 | 995 => Some("POP3"),
        143 | 993 => Some("IMAP"),
        443 | 8443 => Some("HTTPS"),
        445 => Some("SMB"),
        1433 => Some("MSSQL"),
        3306 => Some("MySQL"),
        3389 => Some("RDP"),
        5432 => Some("PostgreSQL"),
        6379 => Some("Redis"),
        27017 => Some("MongoDB"),
        _ => None,
    }
    .map(str::to_string)
}

fn sanitize_banner(bytes: &[u8]) -> Option<String> {
    let value: String = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BANNER_BYTES)])
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                ' '
            }
        })
        .collect();
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!value.is_empty()).then_some(value)
}

fn mysql_version_from_greeting(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 7 || bytes[3] != 0 || bytes[4] != 10 {
        return None;
    }
    let end = bytes[5..].iter().position(|byte| *byte == 0)? + 5;
    let version = std::str::from_utf8(&bytes[5..end]).ok()?.trim();
    (!version.is_empty() && version.len() <= 200).then(|| version.to_string())
}

fn product_version(banner: &str, product: &str) -> Option<String> {
    let start = banner.to_lowercase().find(&product.to_lowercase())?;
    let value = banner[start..]
        .split(|character: char| {
            character == ')' || character == ']' || character == ';' || character == ','
        })
        .next()?
        .trim()
        .chars()
        .take(200)
        .collect::<String>();
    (!value.is_empty()).then_some(value)
}

fn service_from_banner(banner: &str) -> Option<String> {
    let lower = banner.to_lowercase();
    if banner.starts_with("SSH-") {
        Some("SSH")
    } else if lower.contains("ftp") {
        Some("FTP")
    } else if lower.contains("esmtp") || lower.contains("smtp") {
        Some("SMTP")
    } else if banner.starts_with("+PONG")
        || lower.starts_with("-noauth")
        || lower.contains("redis_version")
    {
        Some("Redis")
    } else if banner.starts_with("HTTP/") {
        Some("HTTP")
    } else {
        None
    }
    .map(str::to_string)
}

fn version_from_banner(service: Option<&str>, banner: Option<&str>) -> Option<String> {
    let banner = banner?;
    match service {
        Some("SSH") => banner.strip_prefix("SSH-").and_then(|value| {
            value
                .split_once('-')
                .map(|(_, version)| version.to_string())
        }),
        Some("HTTP" | "HTTPS") => banner
            .split("Server=")
            .nth(1)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        Some("FTP") => ["vsFTPd", "ProFTPD", "FileZilla Server"]
            .iter()
            .find_map(|product| product_version(banner, product)),
        Some("SMTP") => ["Postfix", "Exim", "Sendmail", "Microsoft ESMTP"]
            .iter()
            .find_map(|product| product_version(banner, product)),
        _ => None,
    }
}

fn identify_banner(
    hinted_service: Option<String>,
    bytes: &[u8],
) -> (Option<String>, Option<String>, Option<String>) {
    if let Some(version) = mysql_version_from_greeting(bytes) {
        return (
            Some("MySQL".to_string()),
            Some(version),
            Some("MySQL protocol 10 greeting".to_string()),
        );
    }
    let banner = sanitize_banner(bytes);
    let service = banner
        .as_deref()
        .and_then(service_from_banner)
        .or(hinted_service);
    let version = version_from_banner(service.as_deref(), banner.as_deref());
    (service, version, banner)
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let content_start = lower[start..].find('>')? + start + 1;
    let end = lower[content_start..].find("</title>")? + content_start;
    let title = html[content_start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then(|| title.chars().take(200).collect())
}

fn bounded_text(value: impl ToString, max_bytes: usize) -> String {
    let mut bounded = String::new();
    for character in value.to_string().chars() {
        if bounded.len() + character.len_utf8() > max_bytes {
            break;
        }
        bounded.push(character);
    }
    bounded
}

fn finish_certificate_metadata(
    mut metadata: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    for optional_field in ["subject_alt_names", "issuer", "subject", "serial"] {
        if serde_json::Value::Object(metadata.clone())
            .to_string()
            .len()
            <= 2800
        {
            break;
        }
        metadata.remove(optional_field);
    }
    serde_json::Value::Object(metadata)
}

fn certificate_metadata(der: &[u8]) -> serde_json::Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "sha256".to_string(),
        serde_json::Value::String(hex::encode(Sha256::digest(der))),
    );
    metadata.insert("der_bytes".to_string(), serde_json::json!(der.len()));

    let Ok((_, certificate)) = parse_x509_certificate(der) else {
        return finish_certificate_metadata(metadata);
    };
    let subject = bounded_text(certificate.subject(), 400);
    let issuer = bounded_text(certificate.issuer(), 400);
    metadata.insert(
        "subject".to_string(),
        serde_json::Value::String(subject.clone()),
    );
    metadata.insert(
        "issuer".to_string(),
        serde_json::Value::String(issuer.clone()),
    );
    metadata.insert(
        "serial".to_string(),
        serde_json::Value::String(bounded_text(certificate.raw_serial_as_string(), 200)),
    );
    metadata.insert(
        "not_before".to_string(),
        serde_json::Value::String(certificate.validity().not_before.to_string()),
    );
    metadata.insert(
        "not_after".to_string(),
        serde_json::Value::String(certificate.validity().not_after.to_string()),
    );
    metadata.insert(
        "valid_now".to_string(),
        serde_json::Value::Bool(certificate.validity().is_valid()),
    );
    metadata.insert(
        "self_issued".to_string(),
        serde_json::Value::Bool(subject == issuer),
    );

    let subject_alt_names = certificate
        .subject_alternative_name()
        .ok()
        .flatten()
        .into_iter()
        .flat_map(|extension| extension.value.general_names.iter())
        .filter_map(|name| match name {
            GeneralName::DNSName(value) => Some(bounded_text(value, 160)),
            GeneralName::IPAddress(bytes) if bytes.len() == 4 => {
                Some(std::net::Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string())
            }
            GeneralName::IPAddress(bytes) if bytes.len() == 16 => {
                let octets: [u8; 16] = (*bytes).try_into().ok()?;
                Some(std::net::Ipv6Addr::from(octets).to_string())
            }
            _ => None,
        })
        .take(8)
        .map(serde_json::Value::String)
        .collect::<Vec<_>>();
    if !subject_alt_names.is_empty() {
        metadata.insert(
            "subject_alt_names".to_string(),
            serde_json::Value::Array(subject_alt_names),
        );
    }
    finish_certificate_metadata(metadata)
}

async fn probe_http(
    client: &reqwest::Client,
    target: &str,
    port: u16,
    read_title: bool,
) -> ScannerPortResult {
    let service = if matches!(port, 443 | 8443) {
        "HTTPS"
    } else {
        "HTTP"
    };
    let scheme = service.to_ascii_lowercase();
    let host = if target.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{target}]")
    } else {
        target.to_string()
    };
    let mut result = ScannerPortResult {
        port,
        service: Some(service.to_string()),
        version: None,
        banner: None,
        fingerprint: None,
    };
    let Ok(mut response) = client
        .get(format!("{scheme}://{host}:{port}/"))
        .send()
        .await
    else {
        return result;
    };
    let tls_metadata = response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(reqwest::tls::TlsInfo::peer_certificate)
        .map(certificate_metadata);
    let status = response.status();
    let server = response
        .headers()
        .get(reqwest::header::SERVER)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(200).collect::<String>());
    result.version = server.clone();
    result.banner = Some(match server.as_deref() {
        Some(server) => format!("HTTP {} Server={server}", status.as_u16()),
        None => format!("HTTP {}", status.as_u16()),
    });
    let mut fingerprint = serde_json::Map::new();
    if let Some(metadata) = tls_metadata {
        fingerprint.insert("tls".to_string(), metadata);
    }
    if read_title {
        let mut body = Vec::new();
        while body.len() < MAX_HTTP_BODY_BYTES {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    let remaining = MAX_HTTP_BODY_BYTES - body.len();
                    body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                }
                _ => break,
            }
        }
        if let Some(title) = extract_title(&String::from_utf8_lossy(&body)) {
            fingerprint.insert("http_title".to_string(), serde_json::Value::String(title));
        }
    }
    if !fingerprint.is_empty() {
        result.fingerprint = Some(serde_json::Value::Object(fingerprint));
    }
    result
}

async fn probe_banner(target: &str, port: u16) -> ScannerPortResult {
    let service = known_service(port);
    let mut result = ScannerPortResult {
        port,
        service,
        version: None,
        banner: None,
        fingerprint: None,
    };
    let Ok(Ok(mut stream)) = timeout(CONNECT_TIMEOUT, TcpStream::connect((target, port))).await
    else {
        return result;
    };
    if port == 6379 {
        let _ = timeout(CONNECT_TIMEOUT, stream.write_all(b"PING\r\n")).await;
    }
    let mut bytes = [0_u8; MAX_BANNER_BYTES];
    if let Ok(Ok(read)) = timeout(CONNECT_TIMEOUT, stream.read(&mut bytes)).await {
        let (service, version, banner) = identify_banner(result.service, &bytes[..read]);
        result.service = service;
        result.version = version;
        result.banner = banner;
    }
    result
}

async fn probe_postgresql(target: &str, port: u16) -> ScannerPortResult {
    let mut result = ScannerPortResult {
        port,
        service: Some("PostgreSQL".to_string()),
        version: None,
        banner: None,
        fingerprint: None,
    };
    let Ok(Ok(mut stream)) = timeout(CONNECT_TIMEOUT, TcpStream::connect((target, port))).await
    else {
        return result;
    };
    // PostgreSQL SSLRequest performs no login and receives one S/N response byte.
    let ssl_request = [0_u8, 0, 0, 8, 4, 210, 22, 47];
    if !matches!(
        timeout(CONNECT_TIMEOUT, stream.write_all(&ssl_request)).await,
        Ok(Ok(()))
    ) {
        return result;
    }
    let mut response = [0_u8; 1];
    if let Ok(Ok(_)) = timeout(CONNECT_TIMEOUT, stream.read_exact(&mut response)).await
        && matches!(response[0], b'S' | b'N')
    {
        let tls_supported = response[0] == b'S';
        result.banner = Some(format!(
            "PostgreSQL SSL negotiation: {}",
            if tls_supported {
                "supported"
            } else {
                "not supported"
            }
        ));
        result.fingerprint = Some(serde_json::json!({
            "probe": "postgresql_ssl",
            "tls_supported": tls_supported,
        }));
    }
    result
}

fn infer_os(ports: &[ScannerPortResult]) -> Option<String> {
    let evidence = ports
        .iter()
        .flat_map(|port| [port.version.as_deref(), port.banner.as_deref()])
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if ["ubuntu", "debian", "centos", "red hat", "linux"]
        .iter()
        .any(|marker| evidence.contains(marker))
    {
        Some("Linux（服务证据推断）".to_string())
    } else if ["microsoft-iis", "windows server", "win32"]
        .iter()
        .any(|marker| evidence.contains(marker))
    {
        Some("Windows（服务证据推断）".to_string())
    } else {
        None
    }
}

async fn scan_target(
    target: String,
    ports: &[u16],
    service_detection: bool,
    os_detection: bool,
    site_identify: bool,
    http_client: &reqwest::Client,
) -> ScannerTargetResult {
    let semaphore = Arc::new(Semaphore::new(SCAN_CONCURRENCY));
    let mut scans = JoinSet::new();
    for &port in ports {
        let target = target.clone();
        let semaphore = semaphore.clone();
        scans.spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return None;
            };
            match timeout(CONNECT_TIMEOUT, TcpStream::connect((target.as_str(), port))).await {
                Ok(Ok(_)) => Some(port),
                _ => None,
            }
        });
    }
    let mut open_ports = Vec::new();
    while let Some(result) = scans.join_next().await {
        if let Ok(Some(port)) = result {
            open_ports.push(port);
        }
    }
    open_ports.sort_unstable();
    let mut identified_ports = Vec::with_capacity(open_ports.len());
    if service_detection || site_identify {
        let mut probes = JoinSet::new();
        for &port in &open_ports {
            let target = target.clone();
            let http_client = http_client.clone();
            probes.spawn(async move {
                if matches!(port, 80 | 443 | 8000 | 8008 | 8080 | 8081 | 8443 | 8888) {
                    probe_http(&http_client, &target, port, site_identify).await
                } else if service_detection && port == 5432 {
                    probe_postgresql(&target, port).await
                } else if service_detection {
                    probe_banner(&target, port).await
                } else {
                    ScannerPortResult {
                        port,
                        service: known_service(port),
                        version: None,
                        banner: None,
                        fingerprint: None,
                    }
                }
            });
        }
        while let Some(result) = probes.join_next().await {
            if let Ok(result) = result {
                identified_ports.push(result);
            }
        }
        identified_ports.sort_by_key(|result| result.port);
    } else {
        identified_ports = open_ports
            .iter()
            .copied()
            .map(|port| ScannerPortResult {
                port,
                service: known_service(port),
                version: None,
                banner: None,
                fingerprint: None,
            })
            .collect();
    }
    let os_hint = os_detection.then(|| infer_os(&identified_ports)).flatten();
    let fingerprint = identified_ports.iter().find_map(|port| {
        port.fingerprint
            .as_ref()
            .and_then(|value| value.get("http_title"))
            .and_then(serde_json::Value::as_str)
            .map(|title| format!("HTTP title: {title}"))
    });
    ScannerTargetResult {
        target,
        open_ports,
        ports: identified_ports,
        os_hint,
        fingerprint,
    }
}

async fn execute_claim(claim: &ScannerTaskClaim) -> anyhow::Result<Vec<ScannerTargetResult>> {
    let targets = expand_targets(&claim.task.target)?;
    let http_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .tls_info(true)
        // Do not follow redirects outside the task's explicitly authorized target.
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()?;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        results.push(
            scan_target(
                target,
                &claim.ports,
                claim.task.service_detection,
                claim.task.os_detection,
                claim.task.site_identify,
                &http_client,
            )
            .await,
        );
    }
    Ok(results)
}

async fn complete_task(
    client: &reqwest::Client,
    server: &str,
    node_id: &str,
    token: &str,
    claim: &ScannerTaskClaim,
    outcome: anyhow::Result<Vec<ScannerTargetResult>>,
) -> anyhow::Result<()> {
    let (results, error_message) = match outcome {
        Ok(results) => (results, None),
        Err(error) => (Vec::new(), Some(format!("{error:#}"))),
    };
    client
        .post(format!("{server}/asset-ops/scanner-agents/tasks/complete"))
        .json(&ScannerTaskCompleteRequest {
            node_id: node_id.to_string(),
            token: token.to_string(),
            task_id: claim.task.id.clone(),
            claim_token: claim.claim_token.clone(),
            results,
            error_message,
        })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    if env::args().nth(1).as_deref() != Some("register") {
        bail!("usage: kairos-scan-agent register --server <url> --token <token> --name <hostname>");
    }

    let server = argument("--server")?.trim_end_matches('/').to_string();
    let token = argument("--token")?;
    let hostname = argument("--name")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client
        .post(format!("{server}/asset-ops/scanner-agents/register"))
        .json(&RegisterRequest {
            token: &token,
            agent_version: env!("CARGO_PKG_VERSION"),
            hostname: &hostname,
            ip_address: None,
            capabilities: [
                "host_discovery",
                "port_scan",
                "service_detection",
                "site_identify",
            ],
        })
        .send()
        .await?
        .error_for_status()?
        .json::<Envelope<Registration>>()
        .await?;
    let node_id = response.data.node_id;
    tracing::info!(%node_id, "scanner agent registered");

    let heartbeat_client = client.clone();
    let heartbeat_server = server.clone();
    let heartbeat_node_id = node_id.clone();
    let heartbeat_token = token.clone();
    let heartbeat_interval = response.data.heartbeat_interval_seconds.max(10);
    let active_claim = Arc::new(RwLock::new(None::<(String, String)>));
    let heartbeat_claim = active_claim.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(heartbeat_interval)).await;
            let active = heartbeat_claim.read().await.clone();
            let response = heartbeat_client
                .post(format!(
                    "{heartbeat_server}/asset-ops/scanner-agents/heartbeat"
                ))
                .json(&ScannerHeartbeatRequest {
                    node_id: heartbeat_node_id.clone(),
                    token: heartbeat_token.clone(),
                    task_id: active.as_ref().map(|(task_id, _)| task_id.clone()),
                    claim_token: active.map(|(_, claim_token)| claim_token),
                })
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    tracing::debug!(node_id=%heartbeat_node_id, "heartbeat sent");
                }
                Ok(response) => tracing::warn!(status=%response.status(), "heartbeat rejected"),
                Err(error) => tracing::warn!(%error, "heartbeat failed"),
            }
        }
    });

    let poll_seconds = env::var("SCAN_AGENT_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5)
        .clamp(1, 60);
    loop {
        let claimed = client
            .post(format!("{server}/asset-ops/scanner-agents/tasks/claim"))
            .json(&ScannerTaskClaimRequest {
                node_id: node_id.clone(),
                token: token.clone(),
            })
            .send()
            .await;
        match claimed {
            Ok(response) if response.status().is_success() => {
                let claim = response
                    .json::<Envelope<Option<ScannerTaskClaim>>>()
                    .await?
                    .data;
                if let Some(claim) = claim {
                    tracing::info!(task_id=%claim.task.id, target=%claim.task.target, "scan task claimed");
                    *active_claim.write().await =
                        Some((claim.task.id.clone(), claim.claim_token.clone()));
                    let outcome = execute_claim(&claim).await;
                    if let Err(error) =
                        complete_task(&client, &server, &node_id, &token, &claim, outcome).await
                    {
                        tracing::error!(task_id=%claim.task.id, %error, "failed to report scan task result");
                    } else {
                        tracing::info!(task_id=%claim.task.id, "scan task completed");
                    }
                    *active_claim.write().await = None;
                    continue;
                }
            }
            Ok(response) => tracing::warn!(status=%response.status(), "task claim rejected"),
            Err(error) => tracing::warn!(%error, "task claim failed"),
        }
        tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ScannerPortResult, certificate_metadata, expand_targets, extract_title, identify_banner,
        infer_os, probe_postgresql, sanitize_banner, version_from_banner,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn expands_small_cidr_and_deduplicates_targets() {
        let targets = expand_targets("192.0.2.0/30,192.0.2.1").expect("valid targets");
        assert_eq!(targets, vec!["192.0.2.1", "192.0.2.2"]);
    }

    #[test]
    fn rejects_unbounded_cidr() {
        assert!(expand_targets("10.0.0.0/8").is_err());
    }

    #[test]
    fn sanitizes_and_limits_untrusted_banners() {
        let mut banner = b"SSH-2.0-OpenSSH_9.9\r\n".to_vec();
        banner.extend(std::iter::repeat_n(b'x', 600));
        let sanitized = sanitize_banner(&banner).expect("non-empty banner");
        assert!(!sanitized.contains('\n'));
        assert!(sanitized.len() <= 512);
    }

    #[test]
    fn extracts_service_versions_and_site_titles() {
        assert_eq!(
            version_from_banner(Some("SSH"), Some("SSH-2.0-OpenSSH_9.9")),
            Some("OpenSSH_9.9".to_string())
        );
        assert_eq!(
            extract_title("<html><TITLE class='x'> Kairos  Console </TITLE></html>"),
            Some("Kairos Console".to_string())
        );
        assert_eq!(
            version_from_banner(Some("FTP"), Some("220 (vsFTPd 3.0.5)")),
            Some("vsFTPd 3.0.5".to_string())
        );
        assert_eq!(
            version_from_banner(Some("SMTP"), Some("220 mx.example ESMTP Postfix")),
            Some("Postfix".to_string())
        );
    }

    #[test]
    fn protocol_signatures_override_port_hints() {
        let (service, version, banner) =
            identify_banner(Some("SSH".to_string()), b"220 (vsFTPd 3.0.5)\r\n");
        assert_eq!(service.as_deref(), Some("FTP"));
        assert_eq!(version.as_deref(), Some("vsFTPd 3.0.5"));
        assert_eq!(banner.as_deref(), Some("220 (vsFTPd 3.0.5)"));

        let greeting = [
            0x15, 0, 0, 0, 10, b'8', b'.', b'4', b'.', b'0', 0, b'x', b'x', b'x', b'x',
        ];
        let (service, version, banner) = identify_banner(None, &greeting);
        assert_eq!(service.as_deref(), Some("MySQL"));
        assert_eq!(version.as_deref(), Some("8.4.0"));
        assert_eq!(banner.as_deref(), Some("MySQL protocol 10 greeting"));
    }

    #[test]
    fn extracts_bounded_leaf_certificate_metadata() {
        let rcgen::CertifiedKey { cert, .. } = rcgen::generate_simple_self_signed(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ])
        .expect("generate fixture certificate");
        let metadata = certificate_metadata(cert.der().as_ref());
        assert_eq!(
            metadata
                .get("sha256")
                .and_then(serde_json::Value::as_str)
                .map(str::len),
            Some(64)
        );
        assert_eq!(
            metadata
                .get("valid_now")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            metadata
                .get("subject_alt_names")
                .and_then(serde_json::Value::as_array)
                .map(|values| values.iter().any(|value| value == "localhost")),
            Some(true)
        );
        assert!(metadata.to_string().len() <= 2800);
    }

    #[tokio::test]
    async fn postgresql_probe_uses_read_only_ssl_negotiation() {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind fixture");
        let port = listener.local_addr().expect("fixture address").port();
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept probe");
            let mut request = [0_u8; 8];
            stream
                .read_exact(&mut request)
                .await
                .expect("read SSLRequest");
            assert_eq!(request, [0, 0, 0, 8, 4, 210, 22, 47]);
            stream.write_all(b"S").await.expect("write response");
        });

        let result = probe_postgresql("127.0.0.1", port).await;
        fixture.await.expect("fixture task");
        assert_eq!(result.service.as_deref(), Some("PostgreSQL"));
        assert_eq!(
            result.banner.as_deref(),
            Some("PostgreSQL SSL negotiation: supported")
        );
        assert_eq!(
            result
                .fingerprint
                .as_ref()
                .and_then(|value| value.get("tls_supported"))
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn infers_os_only_from_explicit_service_evidence() {
        let mut port = ScannerPortResult {
            port: 22,
            service: Some("SSH".to_string()),
            version: Some("OpenSSH_9.9 Ubuntu".to_string()),
            banner: None,
            fingerprint: None,
        };
        assert_eq!(
            infer_os(&[port.clone()]).as_deref(),
            Some("Linux（服务证据推断）")
        );
        port.version = Some("OpenSSH_9.9".to_string());
        assert_eq!(infer_os(&[port]), None);
    }
}
