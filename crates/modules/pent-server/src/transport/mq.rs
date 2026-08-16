//! NATS message queue integration for Pent Agent.
//!
//! NATS is notification-only: the database remains the source of truth for
//! run state. NATS subjects wake workers when new work is available; workers
//! always claim runs through the DB-backed [`crate::runtime_policy::claim`].
//!
//! ## Queue tiers
//!
//! | Tier | Subject | Typical workloads |
//! |---|---|---|
//! | `scan.light` | `penta.scan.light` | http_probe, dns_lookup, web_crawl |
//! | `scan.heavy` | `penta.scan.heavy` | nmap, nuclei, dir_fuzz |
//! | `native.fast` | `penta.native.fast` | built-in fast tools |
//! | `browser` | `penta.browser` | browser-automation tools |
//! | `dangerous` | `penta.dangerous` | high-risk tools (manual approval) |
//! | `report` | `penta.report` | report generation |

use async_nats::Client;
use futures_util::StreamExt;

/// NATS-backed event bus for run notifications.
///
/// Falls back to a no-op mode when `NATS_URL` is unset or the connection
/// fails — workers will poll the DB directly.
pub struct NatsEventBus {
    client: Option<Client>,
}

impl NatsEventBus {
    /// Connect to NATS. Returns a no-op bus when `NATS_URL` is unset or the
    /// connection fails, so the system degrades gracefully to DB polling.
    pub async fn connect(url: &str) -> Self {
        match async_nats::connect(url).await {
            Ok(client) => {
                tracing::info!(url, "NATS connected");
                Self {
                    client: Some(client),
                }
            }
            Err(error) => {
                tracing::warn!(url, %error, "NATS unavailable; falling back to DB polling");
                Self { client: None }
            }
        }
    }

    /// Return a handle that can be cloned and sent across tasks.
    pub fn handle(&self) -> NatsHandle {
        NatsHandle {
            client: self.client.clone(),
        }
    }

    /// Publish a notification to a subject. Fire-and-forget; callers must
    /// not depend on delivery for correctness — the DB is authoritative.
    pub async fn publish(&self, subject: &str, payload: &[u8]) {
        if let Some(client) = self.client.clone() {
            if let Err(error) = client
                .publish(subject.to_string(), payload.to_vec().into())
                .await
            {
                tracing::warn!(subject, %error, "NATS publish failed");
            }
        }
    }

    /// Subscribe to a subject. Returns a stream of raw message payloads.
    /// When NATS is unavailable the stream never yields — callers must
    /// fall back to periodic DB polling.
    pub async fn subscribe(&self, subject: &str) -> Result<NatsSubscription, String> {
        let Some(client) = &self.client else {
            return Ok(NatsSubscription::empty());
        };
        let sub = client
            .subscribe(subject.to_string())
            .await
            .map_err(|e| format!("NATS subscribe failed: {e}"))?;
        Ok(NatsSubscription { inner: Some(sub) })
    }
}

/// Cloneable handle to a NATS connection. Cheap to clone — safe to pass
/// to worker tasks.
#[derive(Clone)]
pub struct NatsHandle {
    client: Option<Client>,
}

impl NatsHandle {
    pub async fn publish(&self, subject: &str, payload: &[u8]) {
        if let Some(client) = self.client.clone() {
            if let Err(error) = client
                .publish(subject.to_string(), payload.to_vec().into())
                .await
            {
                tracing::warn!(subject, %error, "NATS publish failed (via handle)");
            }
        }
    }
}

/// NATS subscription — yields message payloads or is empty when NATS is
/// unavailable.
pub struct NatsSubscription {
    inner: Option<async_nats::Subscriber>,
}

impl NatsSubscription {
    fn empty() -> Self {
        Self { inner: None }
    }

    /// Wait for the next message. Returns `None` when the subscription is
    /// dropped or NATS is unavailable (callers fall back to polling).
    pub async fn next(&mut self) -> Option<Vec<u8>> {
        match &mut self.inner {
            Some(sub) => sub.next().await.map(|msg| msg.payload.to_vec()),
            None => {
                // NATS unavailable — never yield; callers must use a
                // tokio::time::interval for DB polling instead.
                std::future::pending::<()>().await;
                unreachable!()
            }
        }
    }
}

/// Pent task subjects organized by queue tier.
pub mod subjects {
    /// Queue-tiered run request subjects. Workers subscribe to the tier they
    /// service; the API publishes to the tier matching the run's workload.
    pub mod queue {
        pub const SCAN_LIGHT: &str = "penta.scan.light.requested";
        pub const SCAN_HEAVY: &str = "penta.scan.heavy.requested";
        pub const NATIVE_FAST: &str = "penta.native.fast.requested";
        pub const BROWSER: &str = "penta.browser.requested";
        pub const DANGEROUS: &str = "penta.dangerous.requested";
        pub const REPORT: &str = "penta.report.requested";
    }

    /// Run lifecycle subjects.
    pub const RUN_REQUESTED: &str = "penta.run.requested";
    pub const RUN_CANCELLED: &str = "penta.run.cancelled";

    /// Approval workflow subjects.
    pub const APPROVAL_DECIDED: &str = "penta.approval.decided";

    /// Evidence created (for downstream consumers).
    pub const EVIDENCE_CREATED: &str = "penta.evidence.created";

    /// Worker heartbeat for liveness monitoring.
    pub const WORKER_HEARTBEAT: &str = "penta.worker.heartbeat";

    /// Map a queue tier string (e.g. "scan.light") to its subject.
    pub fn run_requested_for_tier(tier: &str) -> &'static str {
        match tier {
            "scan.light" => queue::SCAN_LIGHT,
            "scan.heavy" => queue::SCAN_HEAVY,
            "native.fast" => queue::NATIVE_FAST,
            "browser" => queue::BROWSER,
            "dangerous" => queue::DANGEROUS,
            "report" => queue::REPORT,
            _ => {
                tracing::warn!(tier, "unknown queue tier; defaulting to scan.light");
                queue::SCAN_LIGHT
            }
        }
    }
}

/// Classify a tool into a queue tier. Mirrors
/// [`kairos_platform_pent_tooling::dispatcher::dispatch_queue`].
/// but operates on tool names directly rather than risk/tool_type pairs.
pub fn classify_tool_queue(tool_name: &str) -> &'static str {
    match tool_name {
        // Light info gathering
        "http_probe" | "dns_lookup" | "web_crawl" | "web_fingerprint" | "web_url_collect"
        | "js_collect" | "knowledge_search" | "evidence_list" | "evidence_search"
        | "evidence_view" | "traffic_list" | "traffic_view" | "traffic_diff" => "scan.light",
        // Heavier active scanning
        "http_request" | "dir_fuzz" | "js_analyze" | "file_upload_validate" => "scan.light",
        // Heavy scanning
        "tcp_scan" | "scanner_nmap" | "scanner_nuclei" | "traffic_replay" => "scan.heavy",
        _ => "scan.light",
    }
}
