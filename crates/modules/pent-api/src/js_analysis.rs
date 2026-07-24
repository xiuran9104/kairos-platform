//! AI-driven JS analysis and attack planning contracts.
//!
//! These types support the modern pentesting workflow:
//!   1. Fetch all JS files from target
//!   2. Feed JS to AI for deep source-code analysis
//!   3. Generate a targeted attack plan from AI findings
//!   4. Fall back to conventional scanning if the AI pipeline fails

use serde::{Deserialize, Serialize};

// ── JS source collection ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JsSourceType {
    /// `<script src="...">` — external file
    External,
    /// `<script>...</script>` — inline block
    Inline,
    /// `//# sourceMappingURL=...` — source map
    SourceMap,
    /// Webpack / Vite / Rollup dynamic chunk
    Chunk,
    /// Main entry bundle
    Bundle,
}

/// A single JavaScript source file collected from the target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsFile {
    /// Full URL the file was fetched from (or originating page URL for inline).
    pub url: String,
    /// File name extracted from the URL path.
    pub filename: String,
    /// Raw source content.
    pub content: String,
    /// Content length in bytes.
    pub size_bytes: u64,
    /// Where this file was discovered.
    pub source_type: JsSourceType,
    /// SHA-256 of content for deduplication.
    pub content_hash: String,
}

/// A complete collection of JS files gathered from a target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsBundle {
    /// The target base URL.
    pub target_url: String,
    /// Collected files (de-duplicated by content hash).
    pub files: Vec<JsFile>,
    /// Total uncompressed size of all files.
    pub total_size_bytes: u64,
    /// Count of external files fetched.
    pub external_count: u32,
    /// Count of inline blocks captured.
    pub inline_count: u32,
    /// Time spent fetching (milliseconds).
    pub fetch_time_ms: u64,
}

// ── AI analysis output ──────────────────────────────────────────────

/// Structured output from AI analysis of JS source code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsAnalysisResult {
    /// Discovered API endpoints (method, path, params, auth).
    pub api_endpoints: Vec<DiscoveredApi>,
    /// Authentication / authorisation patterns detected.
    pub auth_mechanisms: Vec<AuthPattern>,
    /// Technology stack inferred from the source.
    pub tech_stack: TechStack,
    /// Data models / schemas extracted from the code.
    pub data_models: Vec<DataModel>,
    /// Vulnerability hypotheses the AI wants validated.
    pub potential_vulnerabilities: Vec<VulnHypothesis>,
    /// Hidden or admin-only features the AI noticed.
    pub hidden_features: Vec<String>,
    /// Third-party services / domains referenced.
    pub third_party_services: Vec<String>,
    /// How confident the AI is in this analysis (0.0–1.0).
    pub confidence: f32,
    /// Human-readable summary of findings.
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredApi {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH).
    pub method: String,
    /// URL path template (e.g. /api/users/{id}).
    pub path: String,
    /// Query / body parameter names.
    pub params: Vec<String>,
    /// Whether authentication is required.
    pub auth_required: bool,
    /// What the AI thinks this endpoint does.
    pub description: String,
    /// Content-Type if discernible.
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthPattern {
    /// Mechanism: jwt, session, oauth, apikey, basic, bearer, cookie, none.
    pub mechanism_type: String,
    /// Where tokens / credentials are stored client-side.
    pub token_storage: String,
    /// Refresh / renewal logic if observed.
    pub refresh_logic: Option<String>,
    /// Header name used for auth (e.g. Authorization, X-API-Key).
    pub header_name: Option<String>,
    /// Whether the AI thinks this implementation looks weak.
    pub potential_weaknesses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TechStack {
    /// UI frameworks (React, Vue, Angular, Svelte, etc.).
    pub frameworks: Vec<String>,
    /// Utility / state / routing libraries.
    pub libraries: Vec<String>,
    /// Build tooling (Webpack, Vite, esbuild, etc.).
    pub build_tools: Vec<String>,
    /// Inferred backend / BaaS indicators.
    pub backend_indicators: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataModel {
    /// Model / type name.
    pub name: String,
    /// Field names discovered.
    pub fields: Vec<String>,
    /// Fields that appear to carry sensitive data (passwords, tokens, PII).
    pub sensitive_fields: Vec<String>,
    /// Related API endpoint(s) that transfer this model.
    pub related_apis: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VulnHypothesis {
    /// Category: idor, sqli, xss, ssrf, path_traversal, csrf, file_upload,
    ///           jwt_none_alg, auth_bypass, info_disclosure, open_redirect,
    ///           mass_assignment, rate_limit, cors_misconfig, etc.
    pub category: String,
    /// The API endpoint most relevant to this hypothesis.
    pub related_api: Option<String>,
    /// Confidence (0.0–1.0).
    pub confidence: f32,
    /// Why the AI thinks this might be vulnerable.
    pub reasoning: String,
    /// How the AI suggests validating this hypothesis.
    pub suggested_validation: String,
    /// Parameters / inputs that should be tested.
    pub test_inputs: Vec<String>,
}

// ── AI plan generation ──────────────────────────────────────────────

/// Input fed to the AI plan generator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttackPlanInput {
    /// Target host / base URL.
    pub target: String,
    /// Authorised scope snapshot (immutable).
    pub scope: serde_json::Value,
    /// JS analysis results (None if JS fetch failed).
    pub js_analysis: Option<JsAnalysisResult>,
    /// Previously discovered findings / evidence IDs.
    pub prior_findings: Vec<String>,
    /// Resource budget for this plan.
    pub budget: crate::cognition::PlanBudget,
    /// User-provided objective or constraints.
    pub objective: Option<String>,
}

/// AI-generated attack plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttackPlanOutput {
    /// Ordered stages.
    pub stages: Vec<PlannedStage>,
    /// Why the AI chose this strategy.
    pub strategy_rationale: String,
    /// Overall confidence in the plan (0.0–1.0).
    pub confidence: f32,
    /// Whether the AI thinks JS-based targeting was helpful.
    pub js_driven: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedStage {
    /// Human-readable stage name.
    pub name: String,
    /// What this stage aims to accomplish.
    pub objective: String,
    /// Execution order (lower = earlier).
    pub priority: u32,
    /// Suggested tool capabilities for this stage.
    pub suggested_capabilities: Vec<String>,
    /// How to know when this stage is "done".
    pub success_criteria: serde_json::Value,
    /// Specific tasks within this stage.
    pub tasks: Vec<PlannedTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannedTask {
    /// Task name.
    pub name: String,
    /// Description of what to do.
    pub description: String,
    /// Suggested capability for tool routing.
    pub capability: String,
    /// Specific target for this task.
    pub target: Option<String>,
    /// Priority within the stage.
    pub priority: u32,
    /// Expected outputs.
    pub expected_outputs: Vec<String>,
}

// ── JS Fetcher tool types ───────────────────────────────────────────

/// Arguments for the `js_fetcher` tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsFetcherInput {
    /// Target URL to crawl for JS files.
    pub target_url: String,
    /// Maximum number of external JS files to fetch.
    #[serde(default = "default_max_files")]
    pub max_files: u32,
    /// Maximum total size in bytes to download.
    #[serde(default = "default_max_size")]
    pub max_total_size_bytes: u64,
    /// Whether to follow source map references.
    #[serde(default = "default_true")]
    pub follow_sourcemaps: bool,
    /// Whether to capture inline script blocks.
    #[serde(default = "default_true")]
    pub capture_inline: bool,
    /// HTTP request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_max_files() -> u32 {
    50
}
fn default_max_size() -> u64 {
    50 * 1024 * 1024 // 50 MiB
}
fn default_true() -> bool {
    true
}
fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsFetcherOutput {
    /// The bundle collected.
    pub bundle: JsBundle,
    /// URLs that failed to fetch (with reason).
    pub fetch_errors: Vec<JsFetchError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsFetchError {
    pub url: String,
    pub reason: String,
}

// ── AI prompt templates ─────────────────────────────────────────────

/// Build a system prompt for the JS analysis model call.
pub fn js_analysis_system_prompt() -> &'static str {
    concat!(
        "You are a senior web security researcher performing a white-box review of a target ",
        "application's JavaScript source code. Your goal is to extract actionable intelligence ",
        "for planning a penetration test.\n\n",
        "Analyze the provided JS source files and output a JSON object with these fields:\n",
        "  - api_endpoints: array of { method, path, params[], auth_required, description, content_type? }\n",
        "  - auth_mechanisms: array of { mechanism_type, token_storage, refresh_logic?, header_name?, potential_weaknesses[] }\n",
        "  - tech_stack: { frameworks[], libraries[], build_tools[], backend_indicators[] }\n",
        "  - data_models: array of { name, fields[], sensitive_fields[], related_apis[] }\n",
        "  - potential_vulnerabilities: array of { category, related_api?, confidence, reasoning, suggested_validation, test_inputs[] }\n",
        "  - hidden_features: string[] — admin panels, debug endpoints, feature flags\n",
        "  - third_party_services: string[] — external domains / services referenced\n",
        "  - confidence: float 0-1 how confident you are in this analysis\n",
        "  - summary: brief human-readable summary\n\n",
        "Be specific. For every API endpoint you find, list the exact path and parameters. ",
        "For every vulnerability hypothesis, explain WHY you think it exists and HOW to test it. ",
        "Prioritise findings that are actionable within an authorised penetration test.",
    )
}

/// Build a system prompt for the AI plan generator.
pub fn plan_generation_system_prompt() -> &'static str {
    concat!(
        "You are an attack planning specialist. Given intelligence about a target application ",
        "(gathered from JS source code analysis), produce a structured penetration test plan.\n\n",
        "Output a JSON object with:\n",
        "  - stages: array of { name, objective, priority, suggested_capabilities[], success_criteria, tasks[] }\n",
        "    - tasks: array of { name, description, capability, target?, priority, expected_outputs[] }\n",
        "  - strategy_rationale: explain your overall approach\n",
        "  - confidence: float 0-1\n",
        "  - js_driven: true if the plan was guided by JS analysis\n\n",
        "Rules:\n",
        "  1. Every stage must have a clear, measurable success_criteria.\n",
        "  2. Tasks must map to concrete tool capabilities.\n",
        "  3. Do NOT suggest actions outside the provided scope.\n",
        "  4. Order stages from reconnaissance through validation and reporting.\n",
        "  5. If JS analysis revealed specific APIs, prioritise testing those.\n",
        "  6. If JS analysis found auth weaknesses, include them in an early stage.\n",
        "  7. Do NOT include destructive actions (DELETE, DROP, rm, shutdown) unless explicitly authorised.\n",
        "  8. Respect the budget — do not plan more actions than the budget allows.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_analysis_result_serde_roundtrip() {
        let result = JsAnalysisResult {
            api_endpoints: vec![DiscoveredApi {
                method: "POST".into(),
                path: "/api/login".into(),
                params: vec!["username".into(), "password".into()],
                auth_required: false,
                description: "Login endpoint".into(),
                content_type: Some("application/json".into()),
            }],
            auth_mechanisms: vec![AuthPattern {
                mechanism_type: "jwt".into(),
                token_storage: "localStorage".into(),
                refresh_logic: Some("silent refresh via /api/refresh".into()),
                header_name: Some("Authorization".into()),
                potential_weaknesses: vec!["Token stored in localStorage — XSS risk".into()],
            }],
            tech_stack: TechStack {
                frameworks: vec!["React 18".into()],
                libraries: vec!["axios".into(), "react-router".into()],
                build_tools: vec!["Vite".into()],
                backend_indicators: vec!["/api/ prefix suggests REST API".into()],
            },
            data_models: vec![DataModel {
                name: "User".into(),
                fields: vec!["id".into(), "email".into(), "role".into()],
                sensitive_fields: vec!["email".into()],
                related_apis: vec!["/api/users/{id}".into()],
            }],
            potential_vulnerabilities: vec![VulnHypothesis {
                category: "idor".into(),
                related_api: Some("/api/users/{id}".into()),
                confidence: 0.7,
                reasoning: "User ID is a sequential integer with no ownership check visible".into(),
                suggested_validation: "Request /api/users/1, /api/users/2 with different auth tokens".into(),
                test_inputs: vec!["1".into(), "2".into(), "admin".into()],
            }],
            hidden_features: vec!["/admin/debug".into()],
            third_party_services: vec!["sentry.io".into(), "stripe.com".into()],
            confidence: 0.8,
            summary: "React 18 SPA with JWT auth, 12 API endpoints found".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: JsAnalysisResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.api_endpoints.len(), 1);
        assert_eq!(back.tech_stack.frameworks[0], "React 18");
        assert_eq!(back.confidence, 0.8);
    }

    #[test]
    fn attack_plan_output_serde_roundtrip() {
        let output = AttackPlanOutput {
            stages: vec![PlannedStage {
                name: "Targeted Recon".into(),
                objective: "Probe APIs discovered in JS".into(),
                priority: 1,
                suggested_capabilities: vec!["http_probe".into(), "api_fingerprint".into()],
                success_criteria: serde_json::json!({"endpoints_probed": ">=5"}),
                tasks: vec![PlannedTask {
                    name: "Probe login API".into(),
                    description: "Check /api/login response shape and error messages".into(),
                    capability: "http_probe".into(),
                    target: Some("/api/login".into()),
                    priority: 1,
                    expected_outputs: vec!["response_status".into(), "error_messages".into()],
                }],
            }],
            strategy_rationale: "JS analysis revealed 12 API endpoints with JWT auth".into(),
            confidence: 0.75,
            js_driven: true,
        };
        let json = serde_json::to_string(&output).unwrap();
        let back: AttackPlanOutput = serde_json::from_str(&json).unwrap();
        assert!(back.js_driven);
        assert_eq!(back.stages[0].tasks[0].target.as_deref(), Some("/api/login"));
    }

    #[test]
    fn js_fetcher_input_defaults() {
        let input: JsFetcherInput =
            serde_json::from_str(r#"{"target_url":"https://example.com"}"#).unwrap();
        assert_eq!(input.max_files, 50);
        assert_eq!(input.max_total_size_bytes, 50 * 1024 * 1024);
        assert!(input.follow_sourcemaps);
        assert!(input.capture_inline);
        assert_eq!(input.timeout_seconds, 30);
    }
}
