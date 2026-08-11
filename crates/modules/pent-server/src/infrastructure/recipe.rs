//! YAML tool recipes — parsing, validation, template rendering and the
//! runtime recipe store.
//!
//! Recipes are authored as YAML (see `tools/recipes.yaml`), imported into
//! `pent_tool_definitions` by `import_tool_recipes`, and loaded back into an
//! in-memory store at gateway startup. `tool_registry::prepare_tool` consults
//! the store first and falls back to the hardcoded whitelist when a tool has
//! no executable recipe.
//!
//! Safety rails (kept even though recipes may declare program/args):
//! - no shell: program + args are passed as an execve array;
//! - `program` must be an absolute path under an allowed prefix
//!   (`PENTA_RECIPE_PROGRAM_PREFIXES`, default `/usr/bin/,/usr/local/bin/`);
//! - targets still go through `tool_registry::validate_target` (scope policy,
//!   reserved addresses, NUL/newline/`-` injection);
//! - rendered args reject empty strings and `\0`/`\r`/`\n`.

use super::process::NetworkPolicy;
use super::tool_registry::{PreparedTool, validate_target};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ── YAML schema ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeFile {
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub tool_id: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_manifest_version")]
    pub manifest_version: i32,
    pub namespace: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub output_schema: serde_json::Value,
    #[serde(default = "default_risk_level")]
    pub risk_level: String,
    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,
    #[serde(default)]
    pub supported_target_types: Vec<String>,
    pub executor: RecipeExecutor,
    #[serde(default)]
    pub resource_profile: RecipeResourceProfile,
    #[serde(default = "default_lifecycle_status")]
    pub lifecycle_status: String,
    #[serde(default)]
    pub embedding_document: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeExecutor {
    #[serde(rename = "type", default = "default_executor_type")]
    pub kind: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub target_position: TargetPosition,
    #[serde(default)]
    pub normalize_target: NormalizeTarget,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetPosition {
    #[default]
    Last,
    First,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizeTarget {
    #[default]
    None,
    /// Trim a trailing slash and add `https://` when no scheme is present.
    UrlHttps,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RecipeResourceProfile {
    #[serde(default = "default_cpu_cores")]
    pub cpu_cores: f64,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u64,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

fn default_version() -> String {
    "1.0.0".into()
}
fn default_manifest_version() -> i32 {
    1
}
fn default_source() -> String {
    "builtin".into()
}
fn default_risk_level() -> String {
    "low".into()
}
fn default_approval_policy() -> String {
    "conditional".into()
}
fn default_executor_type() -> String {
    "container".into()
}
fn default_lifecycle_status() -> String {
    "active".into()
}
fn default_cpu_cores() -> f64 {
    0.5
}
fn default_memory_mb() -> u64 {
    256
}
fn default_timeout_seconds() -> u64 {
    300
}

// ── Validation ──────────────────────────────────────────────────────────

const RISK_LEVELS: &[&str] = &["low", "medium", "high", "critical"];
const APPROVAL_POLICIES: &[&str] = &["never", "conditional", "always"];
const SOURCES: &[&str] = &["builtin", "external"];
const EXECUTOR_TYPES: &[&str] = &["container", "host", "builtin"];
const LIFECYCLE_STATUSES: &[&str] = &["draft", "active", "disabled", "deprecated"];

fn allowed_program_prefixes() -> Vec<String> {
    std::env::var("PENTA_RECIPE_PROGRAM_PREFIXES")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|prefix| !prefix.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|prefixes: &Vec<String>| !prefixes.is_empty())
        .unwrap_or_else(|| vec!["/usr/bin/".into(), "/usr/local/bin/".into()])
}

fn validate_tool_id(tool_id: &str) -> Result<(), String> {
    let valid = !tool_id.is_empty()
        && tool_id.len() <= 100
        && tool_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && tool_id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(format!("tool_id '{tool_id}' 格式非法"))
    }
}

fn validate_program(program: &str) -> Result<(), String> {
    if !program.starts_with('/') {
        return Err(format!("program '{program}' 必须是绝对路径"));
    }
    if program.chars().any(char::is_whitespace) || program.contains('\0') {
        return Err(format!("program '{program}' 含非法字符"));
    }
    let prefixes = allowed_program_prefixes();
    if prefixes.iter().any(|prefix| program.starts_with(prefix)) {
        Ok(())
    } else {
        Err(format!(
            "program '{program}' 不在允许前缀内（{}）",
            prefixes.join(", ")
        ))
    }
}

/// Scan a template for `{{...}}` placeholders and check each is known.
fn validate_template(template: &str) -> Result<(), String> {
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let end = rest[start..]
            .find("}}")
            .ok_or_else(|| format!("模板占位符未闭合: {template}"))?;
        let key = rest[start + 2..start + end].trim();
        let known = key == "target"
            || key
                .strip_prefix("arg")
                .is_some_and(|idx| !idx.is_empty() && idx.chars().all(|c| c.is_ascii_digit()));
        if !known {
            return Err(format!("未知占位符 {{{{{key}}}}}"));
        }
        rest = &rest[start + end + 2..];
    }
    if rest.contains("}}") {
        return Err(format!("模板包含未匹配的 '}}': {template}"));
    }
    Ok(())
}

/// Validate a single recipe. Returns an error string describing every rule
/// the recipe violates.
pub fn validate_recipe(recipe: &Recipe) -> Result<(), String> {
    validate_tool_id(&recipe.tool_id)?;
    if recipe.name.trim().is_empty() {
        return Err(format!("{}: name 不能为空", recipe.tool_id));
    }
    if recipe.description.trim().is_empty() {
        return Err(format!("{}: description 不能为空", recipe.tool_id));
    }
    if !RISK_LEVELS.contains(&recipe.risk_level.as_str()) {
        return Err(format!("{}: risk_level 非法", recipe.tool_id));
    }
    if !APPROVAL_POLICIES.contains(&recipe.approval_policy.as_str()) {
        return Err(format!("{}: approval_policy 非法", recipe.tool_id));
    }
    if !SOURCES.contains(&recipe.source.as_str()) {
        return Err(format!("{}: source 非法", recipe.tool_id));
    }
    if !LIFECYCLE_STATUSES.contains(&recipe.lifecycle_status.as_str()) {
        return Err(format!("{}: lifecycle_status 非法", recipe.tool_id));
    }
    if !EXECUTOR_TYPES.contains(&recipe.executor.kind.as_str()) {
        return Err(format!("{}: executor.type 非法", recipe.tool_id));
    }
    if recipe.executor.kind == "builtin" {
        return Ok(());
    }
    let program = recipe
        .executor
        .program
        .as_deref()
        .ok_or_else(|| format!("{}: 可执行配方缺少 program", recipe.tool_id))?;
    validate_program(program).map_err(|e| format!("{}: {e}", recipe.tool_id))?;
    if recipe.executor.args.is_empty() {
        return Err(format!("{}: 可执行配方缺少 args", recipe.tool_id));
    }
    for arg in &recipe.executor.args {
        validate_template(arg).map_err(|e| format!("{}: {e}", recipe.tool_id))?;
    }
    Ok(())
}

/// Parse and validate a YAML recipe file.
pub fn parse_recipe_file(content: &str) -> Result<Vec<Recipe>, String> {
    let file: RecipeFile =
        serde_yaml::from_str(content).map_err(|error| format!("YAML 解析失败: {error}"))?;
    let mut seen = HashMap::new();
    for recipe in &file.recipes {
        validate_recipe(recipe)?;
        if seen.insert(recipe.tool_id.clone(), recipe.version.clone()).is_some() {
            return Err(format!("重复配方: {}@{}", recipe.tool_id, recipe.version));
        }
    }
    Ok(file.recipes)
}

// ── Runtime executable recipes ──────────────────────────────────────────

/// A recipe resolved to an executable form held in the runtime store.
#[derive(Debug, Clone)]
pub struct ExecutableRecipe {
    pub program: &'static str,
    pub container_image: String,
    pub args: Vec<String>,
    pub target_position: TargetPosition,
    pub normalize_target: NormalizeTarget,
}

impl ExecutableRecipe {
    pub fn from_recipe(recipe: &Recipe) -> Option<Self> {
        if recipe.executor.kind == "builtin" {
            return None;
        }
        let program = recipe.executor.program.as_deref()?;
        if recipe.executor.args.is_empty() {
            return None;
        }
        Some(Self::new(
            program,
            recipe
                .executor
                .image
                .clone()
                .filter(|image| !image.is_empty())
                .unwrap_or_else(super::tool_registry::toolkit_image),
            recipe.executor.args.clone(),
            recipe.executor.target_position,
            recipe.executor.normalize_target,
        ))
    }

    fn new(
        program: &str,
        container_image: String,
        args: Vec<String>,
        target_position: TargetPosition,
        normalize_target: NormalizeTarget,
    ) -> Self {
        // Program strings come from validated recipes; the set is bounded, so
        // leaking to obtain a &'static str for PreparedTool is acceptable.
        let program: &'static str = Box::leak(program.to_owned().into_boxed_str());
        Self {
            program,
            container_image,
            args,
            target_position,
            normalize_target,
        }
    }

    /// Build an executable recipe from a `pent_tool_definitions.executor` JSON
    /// document produced by the importer.
    pub fn from_executor_json(executor: &serde_json::Value) -> Option<Self> {
        let program = executor.get("command")?.as_str()?;
        let args: Vec<String> = executor
            .get("args")?
            .as_array()?
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect();
        if args.is_empty() {
            return None;
        }
        let target_position = match executor.get("target_position").and_then(|v| v.as_str()) {
            Some("first") => TargetPosition::First,
            _ => TargetPosition::Last,
        };
        let normalize_target = match executor.get("normalize_target").and_then(|v| v.as_str()) {
            Some("url_https") => NormalizeTarget::UrlHttps,
            _ => NormalizeTarget::None,
        };
        Some(Self::new(
            program,
            executor
                .get("image")
                .and_then(|v| v.as_str())
                .filter(|image| !image.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(super::tool_registry::toolkit_image),
            args,
            target_position,
            normalize_target,
        ))
    }
}

// ── Global store ────────────────────────────────────────────────────────

static RECIPE_STORE: RwLock<Option<Arc<HashMap<String, ExecutableRecipe>>>> = RwLock::new(None);

pub fn set_global_recipes(recipes: HashMap<String, ExecutableRecipe>) {
    let mut guard = RECIPE_STORE.write().expect("recipe store poisoned");
    *guard = Some(Arc::new(recipes));
}

/// Look up the executable recipe for a tool, if one was loaded.
pub fn lookup(tool_name: &str) -> Option<ExecutableRecipe> {
    RECIPE_STORE
        .read()
        .expect("recipe store poisoned")
        .as_ref()
        .and_then(|store| store.get(tool_name).cloned())
}

/// Whether an executable recipe exists for this tool.
pub fn has_executable(tool_name: &str) -> bool {
    RECIPE_STORE
        .read()
        .expect("recipe store poisoned")
        .as_ref()
        .is_some_and(|store| store.contains_key(tool_name))
}

/// Load executable recipes from `pent_tool_definitions` into the global store.
/// Returns the number of recipes loaded.
pub async fn load_from_db(
    pool: &kairos_platform_framework_database::PgPool,
) -> anyhow::Result<usize> {
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT tool_id, executor FROM pent_tool_definitions
         WHERE lifecycle_status = 'active'
           AND executor ? 'args'
           AND executor ? 'command'",
    )
    .fetch_all(pool)
    .await?;
    let mut recipes = HashMap::new();
    for (tool_id, executor) in rows {
        if let Some(recipe) = ExecutableRecipe::from_executor_json(&executor) {
            recipes.insert(tool_id, recipe);
        }
    }
    let count = recipes.len();
    set_global_recipes(recipes);
    Ok(count)
}

// ── Template rendering ──────────────────────────────────────────────────

fn render_arg(template: &str, target: &str, request: &[String]) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find("}}")
            .ok_or_else(|| "模板占位符未闭合".to_string())?;
        let key = rest[start + 2..start + end].trim();
        let value = if key == "target" {
            target.to_string()
        } else if let Some(idx) = key.strip_prefix("arg") {
            let index: usize = idx
                .parse()
                .map_err(|_| format!("占位符下标非法: {{{{{key}}}}}"))?;
            request
                .get(index)
                .ok_or_else(|| format!("占位符 {{{{{key}}}}} 超出参数范围"))?
                .clone()
        } else {
            return Err(format!("未知占位符 {{{{{key}}}}}"));
        };
        out.push_str(&value);
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    if out.is_empty() {
        return Err("参数渲染结果为空".into());
    }
    if out.contains(['\0', '\r', '\n']) {
        return Err("参数渲染结果包含非法字符".into());
    }
    Ok(out)
}

fn normalize_url_target(target: &str) -> String {
    let target = target.trim().trim_end_matches('/');
    if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("https://{target}")
    }
}

/// Render an executable recipe into a `PreparedTool`, applying target
/// extraction, normalization, scope validation and template substitution.
pub fn render_recipe(
    recipe: &ExecutableRecipe,
    request: &[String],
    network_policy: &NetworkPolicy,
) -> Result<PreparedTool, String> {
    let raw_target = match recipe.target_position {
        TargetPosition::First => request.first(),
        TargetPosition::Last => request.last(),
    }
    .map(String::as_str)
    .ok_or_else(|| "工具调用缺少目标".to_string())?;
    let target = match recipe.normalize_target {
        NormalizeTarget::None => raw_target.to_string(),
        NormalizeTarget::UrlHttps => normalize_url_target(raw_target),
    };
    validate_target(&target, network_policy)?;
    let args = recipe
        .args
        .iter()
        .map(|arg| render_arg(arg, &target, request))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedTool {
        program: recipe.program,
        container_image: recipe.container_image.clone(),
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_recipe() -> Recipe {
        Recipe {
            tool_id: "nmap_scan".into(),
            version: "1.0.0".into(),
            manifest_version: 1,
            namespace: "scanner".into(),
            name: "Nmap 端口扫描".into(),
            description: "基于 Nmap 执行端口扫描".into(),
            capabilities: vec!["network".into()],
            tags: vec!["nmap".into()],
            source: "builtin".into(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({}),
            risk_level: "medium".into(),
            approval_policy: "conditional".into(),
            supported_target_types: vec!["hostname".into(), "ipv4".into()],
            executor: RecipeExecutor {
                kind: "container".into(),
                image: None,
                program: Some("/usr/bin/nmap".into()),
                args: vec![
                    "-sT".into(),
                    "-Pn".into(),
                    "--reason".into(),
                    "--".into(),
                    "{{target}}".into(),
                ],
                target_position: TargetPosition::Last,
                normalize_target: NormalizeTarget::None,
            },
            resource_profile: RecipeResourceProfile::default(),
            lifecycle_status: "active".into(),
            embedding_document: String::new(),
        }
    }

    #[test]
    fn parses_and_validates_recipe_yaml() {
        let yaml = r#"
recipes:
  - tool_id: nmap_scan
    namespace: scanner
    name: Nmap 端口扫描
    description: 基于 Nmap 执行端口扫描
    executor:
      type: container
      program: /usr/bin/nmap
      args: ["-sT", "--", "{{target}}"]
"#;
        let recipes = parse_recipe_file(yaml).unwrap();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].tool_id, "nmap_scan");
    }

    #[test]
    fn rejects_unknown_placeholder() {
        let mut recipe = sample_recipe();
        recipe.executor.args.push("{{evil}}".into());
        assert!(validate_recipe(&recipe).is_err());
    }

    #[test]
    fn rejects_program_outside_allowed_prefixes() {
        let mut recipe = sample_recipe();
        recipe.executor.program = Some("/tmp/evil".into());
        assert!(validate_recipe(&recipe).is_err());
    }

    #[test]
    fn rejects_duplicate_tool_ids() {
        let yaml = r#"
recipes:
  - tool_id: nmap_scan
    namespace: scanner
    name: a
    description: a
    executor: {type: container, program: /usr/bin/nmap, args: ["{{target}}"]}
  - tool_id: nmap_scan
    namespace: scanner
    name: b
    description: b
    executor: {type: container, program: /usr/bin/nmap, args: ["{{target}}"]}
"#;
        assert!(parse_recipe_file(yaml).is_err());
    }

    #[test]
    fn renders_target_placeholder_with_scope_validation() {
        let recipe = ExecutableRecipe::from_recipe(&sample_recipe()).unwrap();
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        let prepared = render_recipe(&recipe, &["scan".into(), "example.test".into()], &policy).unwrap();
        assert_eq!(prepared.program, "/usr/bin/nmap");
        assert_eq!(prepared.args.last().unwrap(), "example.test");
        assert!(render_recipe(&recipe, &["scan".into(), "other.test".into()], &policy).is_err());
    }

    #[test]
    fn renders_url_normalization() {
        let mut recipe = sample_recipe();
        recipe.executor.normalize_target = NormalizeTarget::UrlHttps;
        let recipe = ExecutableRecipe::from_recipe(&recipe).unwrap();
        let policy = NetworkPolicy::TargetOnly("example.test".into());
        let prepared = render_recipe(&recipe, &["example.test/".into()], &policy).unwrap();
        assert_eq!(prepared.args.last().unwrap(), "https://example.test");
    }

    #[test]
    fn renders_arg_index_placeholders() {
        let args = vec!["{{arg0}}".into(), "x-{{arg1}}".into()];
        let recipe = ExecutableRecipe::new(
            "/usr/bin/nmap",
            "img".into(),
            args,
            TargetPosition::Last,
            NormalizeTarget::None,
        );
        let policy = NetworkPolicy::Full;
        let prepared = render_recipe(&recipe, &["scan".into(), "t".into()], &policy).unwrap();
        assert_eq!(prepared.args, vec!["scan".to_string(), "x-t".to_string()]);
    }

    #[test]
    fn rejects_out_of_range_arg_placeholder() {
        let recipe = ExecutableRecipe::new(
            "/usr/bin/nmap",
            "img".into(),
            vec!["{{arg5}}".into()],
            TargetPosition::Last,
            NormalizeTarget::None,
        );
        let policy = NetworkPolicy::Full;
        assert!(render_recipe(&recipe, &["t".into()], &policy).is_err());
    }

    #[test]
    fn builtin_recipes_are_not_executable() {
        let mut recipe = sample_recipe();
        recipe.executor.kind = "builtin".into();
        recipe.executor.program = None;
        recipe.executor.args = vec![];
        assert!(validate_recipe(&recipe).is_ok());
        assert!(ExecutableRecipe::from_recipe(&recipe).is_none());
    }

    #[test]
    fn store_lookup_and_has_executable() {
        let mut map = HashMap::new();
        map.insert(
            "nmap_scan".to_string(),
            ExecutableRecipe::from_recipe(&sample_recipe()).unwrap(),
        );
        set_global_recipes(map);
        assert!(has_executable("nmap_scan"));
        assert!(!has_executable("nope"));
        assert!(lookup("nmap_scan").is_some());
        // Reset so other tests fall back to the hardcoded registry.
        set_global_recipes(HashMap::new());
        assert!(!has_executable("nmap_scan"));
    }

    #[test]
    fn repository_recipes_file_is_valid_and_covers_registry_tools() {
        let content = include_str!("../../tools/recipes.yaml");
        let recipes = parse_recipe_file(content).unwrap();
        let ids: std::collections::HashSet<&str> =
            recipes.iter().map(|r| r.tool_id.as_str()).collect();
        // Every tool name handled by the hardcoded prepare_tool whitelist must
        // have a recipe (executable, or builtin metadata for http_request).
        for tool in [
            "tcp_scan", "nmap", "nmap_scan", "http_probe", "http_request",
            "nuclei", "web_vuln_scan",
            "sqli_validate", "xss_validate", "csrf_validate",
            "command_injection_validate", "file_inclusion_validate", "file_upload_validate",
            "afrog", "dns_lookup", "ssh_exec", "sql_query",
            "subfinder", "amass", "dnsx", "masscan", "naabu",
            "ffuf", "gobuster", "nikto", "whatweb", "wafw00f", "httpx",
            "wpscan", "web_wordpress_scan",
            "sqlmap", "sqlmap_scan", "xsser", "commix",
            "testssl", "web_tls_scan", "sslscan", "sslyze", "qsslcaudit",
            "dir_fuzz", "dirsearch", "dirb", "feroxbuster", "wfuzz",
            "hakrawler", "katana", "gau", "httrack", "arjun",
            "dnsrecon", "dnsenum", "fierce",
            "dmitry", "theharvester",
            "enum4linux", "smbmap", "smtp-user-enum", "swaks", "davtest",
            "nbtscan", "netdiscover",
            "web_vuln_scan", "web_wordpress_scan", "wapiti", "skipfish", "lbd",
            "dalfox", "web_xss_scan",
        ] {
            assert!(ids.contains(tool), "recipes.yaml 缺少工具 {tool}");
        }
    }

    #[test]
    fn executor_json_round_trip() {
        let executor = serde_json::json!({
            "type": "container",
            "image": "img",
            "command": "/usr/bin/nmap",
            "args": ["--", "{{target}}"],
            "target_position": "last",
            "normalize_target": "url_https"
        });
        let recipe = ExecutableRecipe::from_executor_json(&executor).unwrap();
        assert_eq!(recipe.program, "/usr/bin/nmap");
        assert_eq!(recipe.normalize_target, NormalizeTarget::UrlHttps);
    }
}
