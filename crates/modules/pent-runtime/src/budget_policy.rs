//! Shared policy for deciding whether runtime budget limits are active.

use serde_json::Value;

const LIMIT_KEYS: &[&str] = &[
    "max_tool_rounds",
    "maxToolRounds",
    "max_duration_seconds",
    "maxDurationSeconds",
    "max_time_seconds",
    "max_requests",
    "maxRequests",
    "max_tokens",
    "maxTokens",
];

/// Budget limits are opt-in by default. An explicit snapshot enables them,
/// while the environment variable can force either unlimited or limited mode.
pub fn budget_limits_enabled(snapshot: &Value) -> bool {
    resolve_budget_limits_enabled(
        snapshot,
        std::env::var("PENTA_UNLIMITED_BUDGETS").ok().as_deref(),
    )
}

fn resolve_budget_limits_enabled(snapshot: &Value, unlimited_override: Option<&str>) -> bool {
    match unlimited_override
        .map(str::trim)
        .map(str::to_ascii_lowercase)
    {
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on") => false,
        Some(value) if matches!(value.as_str(), "0" | "false" | "no" | "off") => true,
        _ => LIMIT_KEYS.iter().any(|key| snapshot.get(key).is_some()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_budget_is_unlimited_by_default() {
        assert!(!resolve_budget_limits_enabled(&serde_json::json!({}), None));
    }

    #[test]
    fn explicit_budget_enables_limits_without_an_override() {
        assert!(resolve_budget_limits_enabled(
            &serde_json::json!({"max_tokens": 500_000}),
            None,
        ));
    }

    #[test]
    fn environment_override_can_force_either_mode() {
        let explicit = serde_json::json!({"max_tokens": 500_000});
        assert!(!resolve_budget_limits_enabled(&explicit, Some("true")));
        assert!(resolve_budget_limits_enabled(
            &serde_json::json!({}),
            Some("false"),
        ));
    }
}
