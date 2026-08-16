//! Pure projection of tool execution results into model observations.

use crate::tool_execution::CoordinatedToolResult;
use crate::tool_result::{NormalizedToolResult, ToolResultStatus};

pub const DEFAULT_MODEL_OBSERVATION_CHARS: usize = 8_000;

#[derive(Debug, Clone, Copy, Default)]
pub struct EffectiveObservationPolicy;

impl EffectiveObservationPolicy {
    pub fn is_effective(
        self,
        tool_name: &str,
        normalized: &NormalizedToolResult,
        content: &str,
    ) -> bool {
        if normalized.status != ToolResultStatus::Succeeded || is_reference_tool(tool_name) {
            return false;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            return !content.trim().is_empty();
        };
        match tool_name {
            "js_analyze" => ["endpoints", "security_hints", "technology_hints"]
                .iter()
                .any(|key| {
                    value
                        .get(key)
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|items| !items.is_empty())
                }),
            "http_probe" => value
                .get("results")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|results| {
                    results.iter().any(|result| {
                        result.get("status").is_some()
                            || result.get("reachable").and_then(serde_json::Value::as_bool)
                                == Some(true)
                    })
                }),
            "dns_lookup" => value
                .pointer("/records/A")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|records| !records.is_empty()),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModelObservationCompactor {
    max_chars: usize,
}

impl ModelObservationCompactor {
    pub const fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }

    pub fn compact(self, content: &str) -> String {
        let char_count = content.chars().count();
        if char_count <= self.max_chars {
            return content.to_string();
        }
        let prefix: String = content.chars().take(self.max_chars).collect();
        format!(
            "{prefix}\n...[模型上下文已截断；完整原始响应已保存为 Evidence，共 {char_count} 字符]"
        )
    }
}

impl Default for ModelObservationCompactor {
    fn default() -> Self {
        Self::new(DEFAULT_MODEL_OBSERVATION_CHARS)
    }
}

#[derive(Debug, Clone)]
pub struct ProjectedObservation {
    pub succeeded: bool,
    pub effective: bool,
    pub normalized: NormalizedToolResult,
    pub model_content: String,
}

impl ProjectedObservation {
    pub fn add_evidence_id(&mut self, evidence_id: String) {
        push_unique(&mut self.normalized.references.evidence_ids, evidence_id);
    }

    pub fn add_traffic_id(&mut self, traffic_id: String) {
        push_unique(&mut self.normalized.references.traffic_ids, traffic_id);
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObservationProjector {
    pub effective_policy: EffectiveObservationPolicy,
    pub compactor: ModelObservationCompactor,
}

impl ObservationProjector {
    pub fn project(self, item: &CoordinatedToolResult) -> ProjectedObservation {
        ProjectedObservation {
            succeeded: item.normalized.status == ToolResultStatus::Succeeded,
            effective: self.effective_policy.is_effective(
                &item.tool_name,
                &item.normalized,
                &item.result.content,
            ),
            normalized: item.normalized.clone(),
            model_content: self.compactor.compact(&item.result.content),
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn is_reference_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "knowledge_search"
            | "evidence_list"
            | "evidence_search"
            | "evidence_view"
            | "traffic_list"
            | "traffic_view"
            | "traffic_sitemap"
            | "traffic_diff"
            | "skill_list"
            | "skill_load"
            | "record_finding"
            | "agent_spawn"
            | "agent_status"
            | "agent_collect"
            | "agent_list"
            | "plan_create"
            | "plan_get"
            | "plan_update"
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::ports::RuntimeToolResult;
    use crate::tool_result::normalize_tool_result;

    use super::*;

    fn item(tool_name: &str, success: bool, content: &str) -> CoordinatedToolResult {
        let result = RuntimeToolResult {
            tool_call_id: "call-1".into(),
            tool_name: tool_name.into(),
            content: content.into(),
            success,
        };
        let normalized = normalize_tool_result(&result);
        CoordinatedToolResult {
            index: 0,
            call_id: "call-1".into(),
            tool_name: tool_name.into(),
            arguments: "{}".into(),
            result,
            normalized,
            elapsed: Duration::ZERO,
            inline: false,
        }
    }

    #[test]
    fn reference_and_blocked_results_are_not_effective() {
        let projector = ObservationProjector::default();
        assert!(
            !projector
                .project(&item(
                    "knowledge_search",
                    true,
                    r#"{"guides":[{"title":"SQL injection"}]}"#,
                ))
                .effective
        );
        assert!(
            !projector
                .project(&item(
                    "dir_fuzz",
                    false,
                    r#"{"blocked":true,"error":"disabled"}"#,
                ))
                .effective
        );
    }

    #[test]
    fn target_observation_requires_meaningful_content() {
        let projector = ObservationProjector::default();
        assert!(
            !projector
                .project(&item(
                    "js_analyze",
                    true,
                    r#"{"endpoints":[],"security_hints":[],"technology_hints":[]}"#,
                ))
                .effective
        );
        assert!(
            projector
                .project(&item(
                    "js_analyze",
                    true,
                    r#"{"endpoints":["/api/users"],"security_hints":[]}"#,
                ))
                .effective
        );
    }

    #[test]
    fn model_observation_is_bounded() {
        let raw = "a".repeat(DEFAULT_MODEL_OBSERVATION_CHARS + 10);
        let compact = ModelObservationCompactor::default().compact(&raw);
        assert!(compact.contains("完整原始响应已保存为 Evidence"));
        assert!(compact.len() < raw.len() + 100);
    }

    #[test]
    fn added_references_are_normalized_and_deduplicated() {
        let mut projected = ObservationProjector::default().project(&item(
            "nmap",
            true,
            r#"{"evidence_id":"ev-1"}"#,
        ));
        projected.add_evidence_id("ev-1".into());
        projected.add_evidence_id("ev-2".into());
        assert_eq!(
            projected.normalized.references.evidence_ids,
            ["ev-1", "ev-2"]
        );
    }
}
