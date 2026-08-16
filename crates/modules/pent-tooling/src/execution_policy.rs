//! Infrastructure-independent execution resilience policies.

use std::{collections::HashMap, time::Instant};

use tokio::sync::Mutex;
use tracing::warn;

/// Per-tool circuit breaker shared by execution adapters.
pub struct CircuitBreaker {
    failures: Mutex<HashMap<String, BreakerState>>,
    threshold: u32,
    recovery_secs: u64,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("threshold", &self.threshold)
            .field("recovery_secs", &self.recovery_secs)
            .finish_non_exhaustive()
    }
}

struct BreakerState {
    failures: u32,
    open_until: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, recovery_secs: u64) -> Self {
        Self {
            failures: Mutex::new(HashMap::new()),
            threshold: threshold.max(1),
            recovery_secs,
        }
    }

    pub async fn check(&self, tool_name: &str) -> Result<(), String> {
        let mut states = self.failures.lock().await;
        let state = states.entry(tool_name.to_string()).or_insert(BreakerState {
            failures: 0,
            open_until: None,
        });

        if let Some(open_until) = state.open_until {
            if Instant::now() < open_until {
                return Err(format!("tool {tool_name} circuit breaker is open"));
            }
            state.open_until = None;
            state.failures = 0;
        }
        Ok(())
    }

    pub async fn record_success(&self, tool_name: &str) {
        self.failures.lock().await.insert(
            tool_name.to_string(),
            BreakerState {
                failures: 0,
                open_until: None,
            },
        );
    }

    pub async fn record_failure(&self, tool_name: &str) {
        let mut states = self.failures.lock().await;
        let state = states.entry(tool_name.to_string()).or_insert(BreakerState {
            failures: 0,
            open_until: None,
        });
        state.failures += 1;
        if state.failures >= self.threshold {
            warn!(tool = %tool_name, threshold = self.threshold, "circuit breaker opened");
            state.open_until =
                Some(Instant::now() + std::time::Duration::from_secs(self.recovery_secs));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opens_at_threshold_and_success_resets_it() {
        let breaker = CircuitBreaker::new(2, 60);
        breaker.record_failure("nmap").await;
        assert!(breaker.check("nmap").await.is_ok());
        breaker.record_failure("nmap").await;
        assert!(breaker.check("nmap").await.is_err());
        breaker.record_success("nmap").await;
        assert!(breaker.check("nmap").await.is_ok());
    }

    #[tokio::test]
    async fn tracks_tools_independently() {
        let breaker = CircuitBreaker::new(1, 60);
        breaker.record_failure("nmap").await;
        assert!(breaker.check("nmap").await.is_err());
        assert!(breaker.check("nuclei").await.is_ok());
    }
}
