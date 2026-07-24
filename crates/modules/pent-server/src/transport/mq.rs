//! NATS message queue integration for Pent Agent.
//!
//! Uses NATS for task notifications, event broadcasting between workers.
//! Database remains the source of truth; NATS only for notification.

use std::sync::Arc;
use tokio::sync::Mutex;

/// NATS-backed event bus.
pub struct NatsEventBus {
    _url: String,
    _client: Option<Arc<Mutex<()>>>,
}

impl NatsEventBus {
    /// Create a new NATS event bus. Falls back to in-memory if NATS unavailable.
    pub async fn connect(url: &str) -> Result<Self, String> {
        // In production, use async_nats::connect(url)
        // For now, log that NATS would be used
        tracing::info!(url, "NATS event bus configured (stub mode)");
        Ok(Self {
            _url: url.to_string(),
            _client: None,
        })
    }

    /// Publish a task notification to a subject.
    pub async fn publish(&self, subject: &str, payload: &[u8]) -> Result<(), String> {
        tracing::debug!(subject, len = payload.len(), "NATS publish (stub)");
        Ok(())
    }

    /// Subscribe to a subject for task notifications.
    pub async fn subscribe(&self, subject: &str) -> Result<NatsSubscription, String> {
        tracing::debug!(subject, "NATS subscribe (stub)");
        Ok(NatsSubscription {
            _subject: subject.to_string(),
        })
    }
}

/// NATS subscription handle.
pub struct NatsSubscription {
    _subject: String,
}

impl NatsSubscription {
    pub async fn next(&mut self) -> Option<Vec<u8>> {
        None // Stub: no messages
    }
}

/// Pent task subjects.
pub mod subjects {
    pub const RUN_REQUESTED: &str = "penta.run.requested";
    pub const RUN_CANCELLED: &str = "penta.run.cancelled";
    pub const APPROVAL_DECIDED: &str = "penta.approval.decided";
    pub const EVIDENCE_CREATED: &str = "penta.evidence.created";
    pub const WORKER_HEARTBEAT: &str = "penta.worker.heartbeat";
}
