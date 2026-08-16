//! Normalization of adapter-owned tool results into runtime semantics.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ports::RuntimeToolResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultReferences {
    pub evidence_ids: Vec<String>,
    pub traffic_ids: Vec<String>,
    pub finding_ids: Vec<String>,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedToolResult {
    pub status: ToolResultStatus,
    pub error: Option<ToolResultError>,
    pub references: ToolResultReferences,
    pub truncated: bool,
}

pub fn normalize_tool_result(result: &RuntimeToolResult) -> NormalizedToolResult {
    let parsed = serde_json::from_str::<Value>(&result.content).ok();
    let blocked = parsed
        .as_ref()
        .and_then(|value| value.get("blocked"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = if blocked {
        ToolResultStatus::Blocked
    } else if result.success {
        ToolResultStatus::Succeeded
    } else {
        ToolResultStatus::Failed
    };
    let error = if status == ToolResultStatus::Succeeded {
        None
    } else {
        Some(normalize_error(parsed.as_ref(), &result.content, blocked))
    };
    let references = parsed.as_ref().map(extract_references).unwrap_or_default();
    let truncated = parsed
        .as_ref()
        .and_then(|value| value.get("truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    NormalizedToolResult {
        status,
        error,
        references,
        truncated,
    }
}

fn normalize_error(parsed: Option<&Value>, raw: &str, blocked: bool) -> ToolResultError {
    let code = parsed
        .and_then(|value| value.get("error_code").or_else(|| value.get("code")))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if blocked {
                "TOOL_BLOCKED".into()
            } else {
                "TOOL_EXECUTION_FAILED".into()
            }
        });
    let message = parsed
        .and_then(|value| value.get("error"))
        .and_then(|error| match error {
            Value::String(message) => Some(message.clone()),
            Value::Object(object) => object
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| raw.chars().take(2_000).collect());
    let retryable = parsed
        .and_then(|value| value.get("retryable"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    ToolResultError {
        code,
        message,
        retryable,
    }
}

fn extract_references(value: &Value) -> ToolResultReferences {
    ToolResultReferences {
        evidence_ids: merged_string_values(value, "evidence_id", "evidence_ids"),
        traffic_ids: merged_string_values(value, "traffic_id", "traffic_ids"),
        finding_ids: merged_string_values(value, "finding_id", "finding_ids"),
        artifacts: string_values(value.get("artifacts")),
    }
}

fn merged_string_values(value: &Value, singular: &str, plural: &str) -> Vec<String> {
    let mut values = string_values(value.get(singular));
    for item in string_values(value.get(plural)) {
        if !values.contains(&item) {
            values.push(item);
        }
    }
    values
}

fn string_values(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values.iter().filter_map(reference_value).collect(),
        Some(value @ Value::Object(_)) => reference_value(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn reference_value(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned).or_else(|| {
        value
            .get("id")
            .or_else(|| value.get("uri"))
            .or_else(|| value.get("path"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(success: bool, content: &str) -> RuntimeToolResult {
        RuntimeToolResult {
            tool_call_id: "call-1".into(),
            tool_name: "tool".into(),
            content: content.into(),
            success,
        }
    }

    #[test]
    fn normalizes_blocked_error_and_references() {
        let normalized = normalize_tool_result(&result(
            false,
            r#"{"blocked":true,"error":"denied","retryable":false,"evidence_id":"ev-1","evidence_ids":["ev-1","ev-2"],"artifacts":[{"uri":"s3://bucket/a"}]}"#,
        ));

        assert_eq!(normalized.status, ToolResultStatus::Blocked);
        assert_eq!(normalized.error.unwrap().code, "TOOL_BLOCKED");
        assert_eq!(normalized.references.evidence_ids, vec!["ev-1", "ev-2"]);
        assert_eq!(normalized.references.artifacts, vec!["s3://bucket/a"]);
    }

    #[test]
    fn normalizes_unstructured_failure() {
        let normalized = normalize_tool_result(&result(false, "process exited with status 1"));

        assert_eq!(normalized.status, ToolResultStatus::Failed);
        assert_eq!(normalized.error.unwrap().code, "TOOL_EXECUTION_FAILED");
    }
}
