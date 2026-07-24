//! Evidence domain logic — ported from PentAGI.

/// Create evidence record.
pub fn create_evidence(url: &str, status: u16, body: &str) -> serde_json::Value {
    serde_json::json!({"url":url,"status":status,"body_preview":&body[..body.len().min(500)],"timestamp":chrono::Utc::now().to_rfc3339()})
}
