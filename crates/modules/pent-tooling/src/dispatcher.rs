//! NATS Worker dispatcher (AI-08). Queues by risk/type profile.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Queue definitions for tool execution.
pub const QUEUE_NATIVE_FAST: &str = "penta.native.fast";
pub const QUEUE_SCAN_LIGHT: &str = "penta.scan.light";
pub const QUEUE_SCAN_HEAVY: &str = "penta.scan.heavy";
pub const QUEUE_BROWSER: &str = "penta.browser";
pub const QUEUE_DANGEROUS: &str = "penta.dangerous";
pub const QUEUE_REPORT: &str = "penta.report";

/// Dispatch a tool invocation to the appropriate queue.
pub fn dispatch_queue(risk_level: &str, tool_type: &str) -> &'static str {
    match risk_level {
        "critical" => QUEUE_DANGEROUS,
        "high" => match tool_type {
            "browser" => QUEUE_BROWSER,
            "scan" => QUEUE_SCAN_HEAVY,
            _ => QUEUE_SCAN_LIGHT,
        },
        "medium" => QUEUE_SCAN_LIGHT,
        _ => QUEUE_NATIVE_FAST,
    }
}

/// Concurrent execution limiter per queue.
pub struct QueueLimiter {
    limits: HashMap<&'static str, Arc<Semaphore>>,
}

impl QueueLimiter {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        limits.insert(QUEUE_NATIVE_FAST, Arc::new(Semaphore::new(16)));
        limits.insert(QUEUE_SCAN_LIGHT, Arc::new(Semaphore::new(8)));
        limits.insert(QUEUE_SCAN_HEAVY, Arc::new(Semaphore::new(4)));
        limits.insert(QUEUE_BROWSER, Arc::new(Semaphore::new(2)));
        limits.insert(QUEUE_DANGEROUS, Arc::new(Semaphore::new(1)));
        limits.insert(QUEUE_REPORT, Arc::new(Semaphore::new(4)));
        Self { limits }
    }

    pub fn acquire(&self, queue: &str) -> Option<tokio::sync::SemaphorePermit<'_>> {
        self.limits.get(queue).and_then(|s| s.try_acquire().ok())
    }
}

impl Default for QueueLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_risk_and_tool_type_to_stable_queues() {
        assert_eq!(dispatch_queue("critical", "scan"), QUEUE_DANGEROUS);
        assert_eq!(dispatch_queue("high", "browser"), QUEUE_BROWSER);
        assert_eq!(dispatch_queue("high", "scan"), QUEUE_SCAN_HEAVY);
        assert_eq!(dispatch_queue("medium", "scan"), QUEUE_SCAN_LIGHT);
        assert_eq!(dispatch_queue("low", "native"), QUEUE_NATIVE_FAST);
    }

    #[test]
    fn dangerous_queue_is_serialized() {
        let limiter = QueueLimiter::new();
        let permit = limiter.acquire(QUEUE_DANGEROUS).unwrap();
        assert!(limiter.acquire(QUEUE_DANGEROUS).is_none());
        drop(permit);
        assert!(limiter.acquire(QUEUE_DANGEROUS).is_some());
    }
}
