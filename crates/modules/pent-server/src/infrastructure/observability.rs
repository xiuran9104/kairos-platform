//! M3 Production: Structured observability — span tracing + metrics.
//!
//! Wraps `tracing` spans around tool execution and LLM calls for
//! OpenTelemetry-compatible distributed tracing. A dedicated `/pent/metrics`
//! endpoint already exists (in lib.rs); this module adds the collector-side
//! instrumentation points.

use std::time::Instant;

/// Record a tool execution span. Returns a guard that completes the span on drop.
pub struct ToolSpan {
    tool_name: String,
    started: Instant,
}

impl ToolSpan {
    pub fn start(tool_name: &str) -> Self {
        tracing::info!(
            tool = tool_name,
            event = "tool.execution.started",
            "tool execution started"
        );
        Self {
            tool_name: tool_name.to_string(),
            started: Instant::now(),
        }
    }

    pub fn complete(self, success: bool, exit_code: i32, output_bytes: usize) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        tracing::info!(
            tool = %self.tool_name,
            event = "tool.execution.completed",
            success,
            exit_code,
            output_bytes,
            elapsed_ms,
            "tool execution completed"
        );
        // Prevent drop from logging twice
        std::mem::forget(self);
    }
}

impl Drop for ToolSpan {
    fn drop(&mut self) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        tracing::warn!(
            tool = %self.tool_name,
            event = "tool.execution.dropped",
            elapsed_ms,
            "tool span dropped without explicit completion — possible error path"
        );
    }
}

/// Record an LLM call span.
pub struct LlmCallSpan {
    model_id: Option<String>,
    started: Instant,
}

impl LlmCallSpan {
    pub fn start(model_id: Option<&str>) -> Self {
        let mid = model_id.map(str::to_string);
        tracing::info!(
            model_id = mid.as_deref().unwrap_or("unknown"),
            event = "llm.call.started",
            "LLM call started"
        );
        Self {
            model_id: mid,
            started: Instant::now(),
        }
    }

    pub fn complete(self, input_tokens: u64, output_tokens: u64, success: bool) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        tracing::info!(
            model_id = %self.model_id.as_deref().unwrap_or("unknown"),
            event = "llm.call.completed",
            input_tokens,
            output_tokens,
            elapsed_ms,
            success,
            "LLM call completed"
        );
        std::mem::forget(self);
    }
}

impl Drop for LlmCallSpan {
    fn drop(&mut self) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        tracing::warn!(
            model_id = %self.model_id.as_deref().unwrap_or("unknown"),
            event = "llm.call.dropped",
            elapsed_ms,
            "LLM span dropped without explicit completion"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_span_complete_records_metrics() {
        let span = ToolSpan::start("nmap");
        span.complete(true, 0, 12345);
    }

    #[test]
    fn llm_span_complete_records_metrics() {
        let span = LlmCallSpan::start(Some("deepseek-v4"));
        span.complete(1024, 512, true);
    }
}
