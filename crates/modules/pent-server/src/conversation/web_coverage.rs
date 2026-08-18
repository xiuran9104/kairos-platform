//! Deterministic endpoint/parameter coverage and validation fan-out.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ValidationKind {
    SqlInjection,
    CrossSiteScripting,
    ServerOrFile,
    AccessControl,
}

impl ValidationKind {
    fn label(self) -> &'static str {
        match self {
            Self::SqlInjection => "SQLi",
            Self::CrossSiteScripting => "XSS",
            Self::ServerOrFile => "服务端/文件",
            Self::AccessControl => "鉴权/业务逻辑",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct EndpointState {
    methods: BTreeSet<String>,
    parameters: BTreeSet<String>,
    validations: BTreeSet<ValidationKind>,
    logic_variants: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub(super) struct WebCoverageLedger {
    endpoints: BTreeMap<String, EndpointState>,
    origins: BTreeSet<String>,
    discovery_performed: bool,
}

impl WebCoverageLedger {
    pub(super) fn observe(
        &mut self,
        tool_name: &str,
        arguments: &str,
        result: &str,
        success: bool,
    ) {
        if !success {
            return;
        }
        if is_discovery_tool(tool_name) {
            self.discovery_performed = true;
        }
        let argument_value = serde_json::from_str::<Value>(arguments).unwrap_or(Value::Null);
        let result_value =
            serde_json::from_str::<Value>(result).unwrap_or(Value::String(result.into()));
        let method = argument_value
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let mut argument_strings = Vec::new();
        let mut result_strings = Vec::new();
        collect_strings(&argument_value, &mut argument_strings);
        collect_strings(&result_value, &mut result_strings);
        argument_strings = expand_endpoint_candidates(argument_strings);
        result_strings = expand_endpoint_candidates(result_strings);
        for value in argument_strings.iter().chain(&result_strings) {
            if let Some((origin, _, _)) = parse_absolute_url(value) {
                self.origins.insert(origin);
            }
        }
        let mut observed_keys = BTreeSet::new();
        self.record_structured_endpoints(&result_value, &method, &mut observed_keys);
        let mut argument_keys = BTreeSet::new();
        for value in &argument_strings {
            if let Some((_, key, parameters)) = parse_absolute_url(&value) {
                self.record_endpoint(&key, &method, parameters, &mut argument_keys);
            } else if value.starts_with('/') && value.len() <= 2_048 {
                let (path, parameters) = parse_path(&value);
                let origins = self.origins.clone();
                for origin in origins {
                    let key = format!("{origin}{path}");
                    self.record_endpoint(&key, &method, parameters.clone(), &mut argument_keys);
                }
            }
        }
        observed_keys.extend(argument_keys.iter().cloned());
        for value in result_strings {
            if let Some((_, key, parameters)) = parse_absolute_url(&value) {
                self.record_endpoint(&key, &method, parameters, &mut observed_keys);
            } else if value.starts_with('/') && value.len() <= 2_048 {
                let (path, parameters) = parse_path(&value);
                let origins = self.origins.clone();
                for origin in origins {
                    let key = format!("{origin}{path}");
                    self.record_endpoint(&key, &method, parameters.clone(), &mut observed_keys);
                }
            }
        }
        let body_parameters = structured_input_keys(&argument_value);
        for key in &argument_keys {
            if let Some(endpoint) = self.endpoints.get_mut(key) {
                endpoint.parameters.extend(body_parameters.iter().cloned());
            }
        }
        if let Some(kind) = validation_kind(tool_name, arguments) {
            let logic_variant =
                (kind == ValidationKind::AccessControl).then(|| logic_variant(&argument_value));
            for key in argument_keys {
                if let Some(endpoint) = self.endpoints.get_mut(&key) {
                    if let Some(variant) = logic_variant.as_ref() {
                        endpoint.logic_variants.insert(variant.clone());
                        if endpoint.logic_variants.len() >= 2 {
                            endpoint.validations.insert(kind);
                        }
                    } else {
                        endpoint.validations.insert(kind);
                    }
                }
            }
        }
    }

    fn record_endpoint(
        &mut self,
        key: &str,
        method: &str,
        parameters: BTreeSet<String>,
        observed: &mut BTreeSet<String>,
    ) {
        if key.len() > 2_048 || is_static_asset(key) {
            return;
        }
        let state = self.endpoints.entry(key.to_string()).or_default();
        state.methods.insert(method.to_string());
        state.parameters.extend(parameters);
        observed.insert(key.to_string());
    }

    fn record_structured_endpoints(
        &mut self,
        value: &Value,
        inherited_method: &str,
        observed: &mut BTreeSet<String>,
    ) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| {
                self.record_structured_endpoints(item, inherited_method, observed)
            }),
            Value::Object(object) => {
                let method = object
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or(inherited_method)
                    .to_ascii_uppercase();
                let parameters = structured_endpoint_parameters(object);
                let endpoint = ["url", "action", "endpoint", "path"]
                    .into_iter()
                    .find_map(|key| object.get(key).and_then(Value::as_str));
                if let Some(endpoint) = endpoint {
                    if let Some((_, key, query_parameters)) = parse_absolute_url(endpoint) {
                        let mut all_parameters = parameters.clone();
                        all_parameters.extend(query_parameters);
                        self.record_endpoint(&key, &method, all_parameters, observed);
                    } else if endpoint.starts_with('/') {
                        let (path, query_parameters) = parse_path(endpoint);
                        let mut all_parameters = parameters.clone();
                        all_parameters.extend(query_parameters);
                        for origin in self.origins.clone() {
                            self.record_endpoint(
                                &format!("{origin}{path}"),
                                &method,
                                all_parameters.clone(),
                                observed,
                            );
                        }
                    }
                }
                for child in object.values() {
                    self.record_structured_endpoints(child, &method, observed);
                }
            }
            _ => {}
        }
    }

    pub(super) fn next_actions_instruction(&self) -> Option<String> {
        let actions = self.missing_actions();
        (!actions.is_empty()).then(|| {
            format!(
                "## 运行时端点覆盖账本\n已发现 {} 个动态端点。以下验证任务由运行时从端点/参数确定性生成，请优先执行，完成后再 FINAL：\n{}",
                self.endpoints.len(),
                actions.into_iter().take(12).map(|item| format!("- {item}")).collect::<Vec<_>>().join("\n")
            )
        })
    }

    pub(super) fn final_rejection(&self) -> Option<String> {
        if !self.discovery_performed {
            return Some("尚未执行 Web 爬取、目录枚举、JS/API 收集，无法证明攻击面已经建立".into());
        }
        if self.endpoints.is_empty() {
            return Some(
                "攻击面工具没有形成可用端点账本，请检查工具输出或改用 web_crawl/web_url_collect"
                    .into(),
            );
        }
        let actions = self.missing_actions();
        (!actions.is_empty()).then(|| {
            format!(
                "端点级验证仍有 {} 项未完成：{}",
                actions.len(),
                actions.into_iter().take(8).collect::<Vec<_>>().join("；")
            )
        })
    }

    fn missing_actions(&self) -> Vec<String> {
        let mut actions = Vec::new();
        for (endpoint, state) in &self.endpoints {
            if !state.parameters.is_empty() {
                for kind in [
                    ValidationKind::SqlInjection,
                    ValidationKind::CrossSiteScripting,
                ] {
                    if !state.validations.contains(&kind) {
                        actions.push(format!(
                            "{} 对 `{endpoint}` 参数 [{}] 尚未验证",
                            kind.label(),
                            state
                                .parameters
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(",")
                        ));
                    }
                }
            }
            if looks_server_or_file_endpoint(endpoint)
                && !state.validations.contains(&ValidationKind::ServerOrFile)
            {
                actions.push(format!("服务端/文件类对 `{endpoint}` 尚未验证"));
            }
            if looks_sensitive_endpoint(endpoint)
                && !state.validations.contains(&ValidationKind::AccessControl)
            {
                actions.push(format!(
                    "匿名/普通用户/其他对象差分对 `{endpoint}` 尚未验证（至少两种身份或对象）"
                ));
            }
        }
        actions
    }
}

fn collect_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => output.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_strings(item, output)),
        Value::Object(object) => object
            .values()
            .for_each(|item| collect_strings(item, output)),
        _ => {}
    }
}

fn expand_endpoint_candidates(strings: Vec<String>) -> Vec<String> {
    let url_pattern = Regex::new(r#"(?i)https?://[^\s"'<>\]]+"#).ok();
    let mut expanded = BTreeSet::new();
    for text in strings {
        expanded.insert(text.clone());
        if let Some(pattern) = &url_pattern {
            expanded.extend(pattern.find_iter(&text).map(|value| {
                value
                    .as_str()
                    .trim_end_matches([',', ';', ')', '.'])
                    .to_string()
            }));
        }
        expanded.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| line.starts_with('/') && line.len() <= 2_048)
                .map(str::to_string),
        );
    }
    expanded.into_iter().collect()
}

fn parse_absolute_url(value: &str) -> Option<(String, String, BTreeSet<String>)> {
    let regex = Regex::new(r#"(?i)^(https?)://([^/\s?#]+)([^\s#]*)"#).ok()?;
    let captures = regex.captures(value.trim_matches(['"', '\'', '<', '>']))?;
    let origin = format!(
        "{}://{}",
        captures.get(1)?.as_str().to_ascii_lowercase(),
        captures.get(2)?.as_str().to_ascii_lowercase()
    );
    let (path, parameters) = parse_path(captures.get(3).map(|value| value.as_str()).unwrap_or("/"));
    Some((origin.clone(), format!("{origin}{path}"), parameters))
}

fn parse_path(value: &str) -> (String, BTreeSet<String>) {
    let (path, query) = value.split_once('?').unwrap_or((value, ""));
    let path = if path.is_empty() { "/" } else { path };
    let parameters = query
        .split('&')
        .filter_map(|pair| pair.split('=').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    let normalized = canonicalize_dynamic_path(&path.trim_end_matches('/').replace("//", "/"));
    (
        if normalized.is_empty() {
            "/".into()
        } else {
            normalized
        },
        parameters,
    )
}

fn structured_input_keys(value: &Value) -> BTreeSet<String> {
    ["query", "form", "json", "body", "data"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(Value::as_object))
        .flat_map(|object| object.keys().cloned())
        .collect()
}

fn structured_endpoint_parameters(object: &serde_json::Map<String, Value>) -> BTreeSet<String> {
    let mut parameters = BTreeSet::new();
    for key in ["parameters", "params", "fields", "inputs"] {
        match object.get(key) {
            Some(Value::Array(values)) => {
                parameters.extend(values.iter().filter_map(Value::as_str).map(str::to_string))
            }
            Some(Value::Object(values)) => parameters.extend(values.keys().cloned()),
            _ => {}
        }
    }
    parameters
}

fn validation_kind(tool_name: &str, arguments: &str) -> Option<ValidationKind> {
    match tool_name {
        "sqlmap" | "sqlmap_scan" | "sqli_validate" => Some(ValidationKind::SqlInjection),
        "dalfox" | "web_xss_scan" | "xss_validate" | "xsser" => {
            Some(ValidationKind::CrossSiteScripting)
        }
        "command_injection_validate"
        | "file_inclusion_validate"
        | "file_upload_validate"
        | "commix" => Some(ValidationKind::ServerOrFile),
        "http_request" | "http-framework-test" | "traffic_replay"
            if super::scan_progress::looks_like_logic_validation(arguments) =>
        {
            Some(ValidationKind::AccessControl)
        }
        "http_request" | "http-framework-test" | "traffic_replay"
            if super::scan_progress::looks_like_server_or_file_validation(arguments) =>
        {
            Some(ValidationKind::ServerOrFile)
        }
        _ => None,
    }
}

fn logic_variant(arguments: &Value) -> String {
    let mut values = Vec::new();
    collect_identity_values(arguments, "", &mut values);
    collect_object_references(arguments, &mut values);
    if values.is_empty() {
        "anonymous".into()
    } else {
        values.sort();
        values.dedup();
        values.join("|")
    }
}

fn canonicalize_dynamic_path(path: &str) -> String {
    let uuid = Regex::new(
        r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    )
    .ok();
    path.split('/')
        .map(|segment| {
            if !segment.is_empty()
                && (segment.chars().all(|character| character.is_ascii_digit())
                    || (segment.len() >= 16
                        && segment
                            .chars()
                            .all(|character| character.is_ascii_hexdigit()))
                    || uuid
                        .as_ref()
                        .is_some_and(|pattern| pattern.is_match(segment)))
            {
                "{id}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_object_references(value: &Value, output: &mut Vec<String>) {
    let mut strings = Vec::new();
    collect_strings(value, &mut strings);
    let path_id = Regex::new(r"/(\d+)(?:/|\?|$)").ok();
    for text in strings {
        if let Some(pattern) = &path_id {
            output.extend(
                pattern
                    .captures_iter(&text)
                    .filter_map(|capture| capture.get(1))
                    .map(|value| format!("object={}", value.as_str())),
            );
        }
        let query = text.split_once('?').map(|(_, query)| query).unwrap_or("");
        for pair in query.split('&') {
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            if [
                "id", "user", "account", "order", "invoice", "tenant", "object",
            ]
            .iter()
            .any(|marker| name.to_ascii_lowercase().contains(marker))
                && !value.is_empty()
            {
                output.push(format!("{}={value}", name.to_ascii_lowercase()));
            }
        }
    }
}

fn collect_identity_values(value: &Value, key: &str, output: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (child_key, child) in object {
                let lower = child_key.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "cookie" | "authorization" | "token" | "user_id" | "account_id" | "tenant_id"
                ) {
                    output.push(format!("{lower}={child}"));
                } else {
                    collect_identity_values(child, child_key, output);
                }
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_identity_values(item, key, output)),
        _ if matches!(
            key.to_ascii_lowercase().as_str(),
            "cookie" | "authorization" | "token"
        ) =>
        {
            output.push(format!("{}={value}", key.to_ascii_lowercase()));
        }
        _ => {}
    }
}

fn is_discovery_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "web_crawl"
            | "web_url_collect"
            | "js_collect"
            | "js_analyze"
            | "dir_fuzz"
            | "ffuf"
            | "gobuster"
            | "feroxbuster"
            | "dirsearch"
            | "arjun"
            | "paramspider"
            | "hakrawler"
    )
}

fn is_static_asset(endpoint: &str) -> bool {
    [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".woff", ".ico",
    ]
    .iter()
    .any(|suffix| endpoint.to_ascii_lowercase().ends_with(suffix))
}

fn looks_sensitive_endpoint(endpoint: &str) -> bool {
    [
        "admin", "user", "account", "profile", "order", "invoice", "internal", "auth", "role",
        "tenant",
    ]
    .iter()
    .any(|marker| endpoint.to_ascii_lowercase().contains(marker))
}

fn looks_server_or_file_endpoint(endpoint: &str) -> bool {
    [
        "fetch", "url", "proxy", "redirect", "file", "upload", "download", "ping", "exec",
        "include", "template",
    ]
    .iter()
    .any(|marker| endpoint.to_ascii_lowercase().contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_endpoint_parameter_and_logic_matrix() {
        let mut ledger = WebCoverageLedger::default();
        ledger.observe(
            "web_crawl",
            r#"{"url":"http://target/"}"#,
            r#"{"endpoints":["/users/1?id=1","/fetch?url=x"]}"#,
            true,
        );
        let rejection = ledger.final_rejection().unwrap();
        assert!(rejection.contains("SQLi"));
        assert!(rejection.contains("身份或对象"));
        assert!(rejection.contains("服务端/文件"));

        ledger.observe(
            "sqlmap_scan",
            r#"{"url":"http://target/users/1?id=1"}"#,
            "{}",
            true,
        );
        ledger.observe(
            "web_xss_scan",
            r#"{"url":"http://target/users/1?id=1"}"#,
            "{}",
            true,
        );
        ledger.observe("http_request", r#"{"url":"http://target/users/1?id=1","method":"POST","headers":{"Cookie":"session=a"}}"#, "{}", true);
        assert!(ledger.final_rejection().is_some());
        ledger.observe("http_request", r#"{"url":"http://target/users/1?id=1","method":"POST","headers":{"Cookie":"session=b"}}"#, "{}", true);
        ledger.observe(
            "file_inclusion_validate",
            r#"{"url":"http://target/fetch?url=x"}"#,
            "{}",
            true,
        );
        ledger.observe(
            "sqlmap_scan",
            r#"{"url":"http://target/fetch?url=x"}"#,
            "{}",
            true,
        );
        ledger.observe(
            "web_xss_scan",
            r#"{"url":"http://target/fetch?url=x"}"#,
            "{}",
            true,
        );
        assert!(ledger.final_rejection().is_none());
    }

    #[test]
    fn associates_structured_form_parameters_with_their_endpoint() {
        let mut ledger = WebCoverageLedger::default();
        ledger.observe(
            "web_crawl",
            r#"{"url":"https://target/"}"#,
            r#"{"forms":[{"url":"https://target/login","method":"POST","parameters":["username","password"]}]}"#,
            true,
        );
        let state = ledger.endpoints.get("https://target/login").unwrap();
        assert!(state.methods.contains("POST"));
        assert_eq!(
            state.parameters,
            BTreeSet::from(["password".to_string(), "username".to_string()])
        );
    }

    #[test]
    fn canonicalizes_object_ids_and_accepts_two_object_variants() {
        let mut ledger = WebCoverageLedger::default();
        for id in ["1001", "1002"] {
            ledger.observe(
                "http_request",
                &format!(
                    r#"{{"url":"https://target/api/users/{id}","headers":{{"Cookie":"session=user"}}}}"#
                ),
                "{}",
                true,
            );
        }
        let state = ledger
            .endpoints
            .get("https://target/api/users/{id}")
            .unwrap();
        assert_eq!(state.logic_variants.len(), 2);
        assert!(state.validations.contains(&ValidationKind::AccessControl));
    }

    #[test]
    fn parses_every_url_from_line_oriented_crawler_stdout() {
        let mut ledger = WebCoverageLedger::default();
        ledger.observe(
            "hakrawler",
            r#"{"target":"https://target/"}"#,
            r#"{"stdout":"https://target/api/users?id=1\nhttps://target/search?q=test\nhttps://target/admin"}"#,
            true,
        );
        assert!(ledger.endpoints.contains_key("https://target/api/users"));
        assert!(ledger.endpoints.contains_key("https://target/search"));
        assert!(ledger.endpoints.contains_key("https://target/admin"));
    }
}
