use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const STALL_WARNING_THRESHOLD: usize = 3;
const STALL_STOP_THRESHOLD: usize = 8;
const MIN_EVIDENCE_CATEGORY_DIVERSITY: usize = 2;
/// Consecutive failures of the same tool that trigger a strategy-switch
/// prompt (and removal of that tool from the model's catalog).
const TOOL_FAILURE_SWITCH_THRESHOLD: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WebCoverageArea {
    SurfaceMapping,
    TemplateScan,
    SqlInjection,
    CrossSiteScripting,
    ServerAndFile,
    AccessAndBusinessLogic,
}

impl WebCoverageArea {
    const REQUIRED: [Self; 6] = [
        Self::SurfaceMapping,
        Self::TemplateScan,
        Self::SqlInjection,
        Self::CrossSiteScripting,
        Self::ServerAndFile,
        Self::AccessAndBusinessLogic,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::SurfaceMapping => "攻击面枚举（爬虫/目录/JS/API 参数）",
            Self::TemplateScan => "通用漏洞模板扫描（nuclei/web_vuln_scan）",
            Self::SqlInjection => "SQL 注入专项验证（sqlmap/sqli_validate）",
            Self::CrossSiteScripting => "XSS 专项验证（dalfox/xss_validate）",
            Self::ServerAndFile => "服务端注入与文件类验证",
            Self::AccessAndBusinessLogic => "鉴权、越权与业务逻辑的差分请求验证",
        }
    }
}

const DVWA_MODULES: [(&str, &str); 14] = [
    ("brute-force", "/vulnerabilities/brute/"),
    ("command-injection", "/vulnerabilities/exec/"),
    ("csrf", "/vulnerabilities/csrf/"),
    ("file-inclusion", "/vulnerabilities/fi/"),
    ("file-upload", "/vulnerabilities/upload/"),
    ("insecure-captcha", "/vulnerabilities/captcha/"),
    ("sql-injection", "/vulnerabilities/sqli/"),
    ("blind-sql-injection", "/vulnerabilities/sqli_blind/"),
    ("weak-session-id", "/vulnerabilities/weak_id/"),
    ("dom-xss", "/vulnerabilities/xss_d/"),
    ("reflected-xss", "/vulnerabilities/xss_r/"),
    ("stored-xss", "/vulnerabilities/xss_s/"),
    ("csp-bypass", "/vulnerabilities/csp/"),
    ("javascript", "/vulnerabilities/javascript/"),
];

/// A module is covered only after a successful, targeted validation attempt.
/// Merely opening the DVWA page is reconnaissance and must not satisfy the
/// completion gate.
fn is_targeted_dvwa_validation(module: &str, tool_name: &str, arguments: &str) -> bool {
    if !matches!(
        tool_name,
        "http_request"
            | "http-framework-test"
            | "traffic_replay"
            | "command_injection_validate"
            | "csrf_validate"
            | "file_inclusion_validate"
            | "file_upload_validate"
            | "sqlmap"
            | "sqlmap_scan"
            | "sqli_validate"
            | "dalfox"
            | "web_xss_scan"
            | "xss_validate"
            | "xsser"
    ) {
        return false;
    }
    let compact = arguments
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let is_post = compact.contains(r#""method":"post""#);
    let has = |needles: &[&str]| needles.iter().any(|needle| compact.contains(needle));
    let xss_scanner = matches!(
        tool_name,
        "dalfox" | "web_xss_scan" | "xss_validate" | "xsser"
    );
    let xss_payload = has(&[
        "%3cscript",
        "<script",
        "%3csvg",
        "<svg",
        "onerror",
        "onload",
        "javascript%3a",
        "javascript:",
    ]);

    match module {
        "brute-force" => has(&["username", "password"]) && has(&["login", "submit"]),
        "command-injection" => {
            tool_name == "command_injection_validate"
                || (has(&[r#""ip""#, "ip="])
                    && has(&["%3b", ";", "%26%26", "&&", "%7c", "|", "%0a"]))
        }
        "csrf" => {
            tool_name == "csrf_validate"
                || (has(&["password_new", "password_conf"]) && has(&["change", "submit"]))
        }
        "file-inclusion" => {
            tool_name == "file_inclusion_validate"
                || (has(&["page="]) && has(&["../", "..%2f", "%2e%2e", "file%3a", "php%3a"]))
        }
        "file-upload" => {
            tool_name == "file_upload_validate"
                || (is_post && has(&["file", "upload", "multipart", "content"]))
        }
        "insecure-captcha" => {
            is_post
                && has(&[
                    "password_new",
                    "password_conf",
                    "g-recaptcha-response",
                    r#""step""#,
                    "step=",
                ])
        }
        "sql-injection" | "blind-sql-injection" => {
            matches!(tool_name, "sqlmap" | "sqlmap_scan" | "sqli_validate")
                || (has(&["id="])
                    && has(&[
                        "%27",
                        "'",
                        "union",
                        "%20or%20",
                        "+or+",
                        "%20and%20",
                        "+and+",
                        "sleep",
                        "benchmark",
                    ]))
        }
        "weak-session-id" => is_post || has(&["generate", "new+session", "new%20session"]),
        "dom-xss" => xss_scanner || (has(&["default="]) && xss_payload),
        "reflected-xss" => xss_scanner || (has(&["name="]) && xss_payload),
        "stored-xss" => {
            xss_scanner || (is_post && has(&["message", "txtname", "mtxmessage"]) && xss_payload)
        }
        "csp-bypass" => has(&["include=", "script="]) && has(&["http", "data%3a", "alert"]),
        "javascript" => (is_post || has(&["submit"])) && has(&["token", "phrase", "success"]),
        _ => false,
    }
}

pub(super) fn looks_like_logic_validation(arguments: &str) -> bool {
    let identity_or_object = [
        "authorization",
        "cookie",
        "session",
        "user_id",
        "userid",
        "account",
        "role",
        "admin",
        "/users/",
        "/user/",
        "/api/internal",
        "object_id",
        "order_id",
        "tenant",
    ]
    .iter()
    .any(|marker| arguments.contains(marker));
    let active_request = [
        r#""method":"post""#,
        r#""method":"put""#,
        r#""method":"patch""#,
        r#""method":"delete""#,
        "headers",
        "cookies",
        "body",
    ]
    .iter()
    .any(|marker| arguments.contains(marker));
    identity_or_object && active_request
}

pub(super) fn looks_like_server_or_file_validation(arguments: &str) -> bool {
    [
        "../",
        "%2e%2e",
        "%2fetc%2f",
        "file://",
        "php://",
        "127.0.0.1",
        "localhost",
        "169.254.169.254",
        "url=",
        "redirect=",
        "next=",
        "callback=",
        "multipart",
        "filename",
        "%3b",
        "&&",
        "%7c",
        "|id",
        "|whoami",
    ]
    .iter()
    .any(|marker| arguments.contains(marker))
}

/// Evidence maturity level required for verified claims in a FINAL.
/// Defaults to E2 (verbatim projection); set `PENTA_MIN_EVIDENCE_MATURITY=E3`
/// to require cross-tool corroboration.
fn min_evidence_maturity() -> u8 {
    match std::env::var("PENTA_MIN_EVIDENCE_MATURITY")
        .unwrap_or_else(|_| "E2".to_string())
        .to_uppercase()
        .as_str()
    {
        "E4" => 4,
        "E3" => 3,
        _ => 2, // E2 is the default
    }
}

/// Tool categories for progress tracking. The runtime does not prescribe a
/// fixed sequence — categories only inform the completion gate and anti-stall
/// diagnostics so the model receives actionable feedback instead of phase
/// enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ToolCategory {
    Recon,
    VulnValidation,
    Knowledge,
    EvidenceRead,
    Unknown,
}

impl ToolCategory {
    fn classify(tool_name: &str) -> Self {
        match tool_name {
            // Reconnaissance tools
            "http_request" | "http_probe" | "dns_lookup" | "web_crawl" | "web_fingerprint"
            | "web_url_collect" | "js_collect" | "nmap" | "nmap_scan" | "tcp_scan" | "masscan"
            | "naabu" | "subfinder" | "amass" | "dnsx" | "dnsrecon" | "dnsenum" | "fierce"
            | "dmitry" | "theharvester" | "nikto" | "whatweb" | "wafw00f" | "wpscan"
            | "web_wordpress_scan" | "whois" | "whois_lookup" | "tls_inspect" | "testssl"
            | "web_tls_scan" | "sslyze" | "qsslcaudit" | "sslscan" | "enum4linux" | "smbmap"
            | "smtp-user-enum" | "swaks" | "davtest" | "nbtscan" | "netdiscover" | "lbd"
            | "hakrawler" | "httrack" | "arjun" | "url_extract" | "url_grep" => Self::Recon,
            // Vulnerability validation tools
            "dir_fuzz"
            | "js_analyze"
            | "file_upload_validate"
            | "nuclei"
            | "web_vuln_scan"
            | "sqli_validate"
            | "xss_validate"
            | "csrf_validate"
            | "command_injection_validate"
            | "file_inclusion_validate"
            | "sqlmap"
            | "sqlmap_scan"
            | "xsser"
            | "dalfox"
            | "web_xss_scan"
            | "commix"
            | "afrog"
            | "wapiti"
            | "skipfish"
            | "ffuf"
            | "gobuster"
            | "feroxbuster"
            | "wfuzz"
            | "dirsearch"
            | "dirb" => Self::VulnValidation,
            "knowledge_search" => Self::Knowledge,
            "evidence_list"
            | "evidence_search"
            | "evidence_view"
            | "query_execution_result"
            | "query-execution-result" => Self::EvidenceRead,
            _ => Self::Unknown,
        }
    }

    /// Mapping to corroboration category for evidence maturity (E3 gate).
    /// Only Recon and VulnValidation count; reference tools do not.
    fn maturity_category(&self) -> Option<&'static str> {
        match self {
            Self::Recon => Some("recon"),
            Self::VulnValidation => Some("validation"),
            _ => None,
        }
    }
}

/// Lightweight progress state for the model-led tool loop.
///
/// This deliberately does not prescribe reconnaissance phases or mandatory
/// tools. It only records whether the model is producing new observations,
/// repeating requests/results, and grounding verified claims in Evidence.
pub(super) struct ScanProgress {
    security_objective: bool,
    broad_web_objective: bool,
    web_coverage: HashSet<WebCoverageArea>,
    seen_results: HashSet<String>,
    tool_calls: usize,
    successful_observations: usize,
    failed_calls: usize,
    unchanged_results: usize,
    consecutive_no_progress: usize,
    /// Distinct tool categories that produced effective observations.
    evidence_categories: HashSet<ToolCategory>,
    /// Tracks which distinct tools have been used successfully.
    tools_used: HashSet<String>,
    /// evidence_id → tool_name mapping for maturity gate (E3 cross-tool check).
    evidence_tools: HashMap<String, String>,
    /// Whether traffic_replay was used successfully (E3 corroboration).
    traffic_replay_count: usize,
    /// Per-tool consecutive failure count so the runtime can temporarily
    /// remove exhausted tools from the model's catalog.
    consecutive_tool_failures: HashMap<String, usize>,
    /// Tools that have been temporarily removed from the catalog due to
    /// consecutive failures. Cleared when any different tool succeeds.
    exhausted_tools: HashSet<String>,
    /// Model-facing strategy-switch hints queued when a tool crosses
    /// [`TOOL_FAILURE_SWITCH_THRESHOLD`]; drained once per round by the
    /// runtime and pushed into `recent_messages`.
    switch_hints: Vec<String>,
    dvwa_objective: bool,
    dvwa_require_findings: bool,
    dvwa_covered: HashSet<String>,
    dvwa_validation_attempts: HashMap<String, usize>,
    dvwa_findings: HashSet<String>,
}

impl ScanProgress {
    pub(super) fn new(user_input: &str) -> Self {
        let normalized = user_input.to_ascii_lowercase();
        // Only treat a turn as a security objective when it carries an
        // explicit target URL or an action-oriented testing command.
        // Questions about findings (e.g. "就是说没有漏洞？") are
        // conversational follow-ups — the model should answer freely
        // without the strict tool/evidence completion gates.
        let has_target = ["http://", "https://"]
            .iter()
            .any(|p| normalized.contains(p));
        let has_test_command = [
            "扫描",
            "渗透",
            "安全测试",
            "复测",
            "scan",
            "pentest",
            "vulnerability",
        ]
        .iter()
        .any(|p| normalized.contains(p));
        let security_objective = has_target || has_test_command;
        let focused_only = [
            "只测xss",
            "只测试xss",
            "只测sql",
            "只测试sql",
            "only xss",
            "only sqli",
            "仅验证",
            "复测这个",
            "retest this",
        ]
        .iter()
        .any(|marker| normalized.contains(marker));
        let dvwa_objective =
            normalized.contains("dvwa") || normalized.contains("damn vulnerable web application");
        let broad_web_objective =
            has_target && has_test_command && !focused_only && !dvwa_objective;
        let dvwa_require_findings = dvwa_objective
            && (normalized.contains("security=low")
                || normalized.contains("security = low")
                || normalized.contains("security low")
                || normalized.contains("低安全级别")
                || normalized.contains("低安全模式"));
        Self {
            security_objective,
            broad_web_objective,
            web_coverage: HashSet::new(),
            seen_results: HashSet::new(),
            tool_calls: 0,
            successful_observations: 0,
            failed_calls: 0,
            unchanged_results: 0,
            consecutive_no_progress: 0,
            evidence_categories: HashSet::new(),
            tools_used: HashSet::new(),
            evidence_tools: HashMap::new(),
            traffic_replay_count: 0,
            consecutive_tool_failures: HashMap::new(),
            exhausted_tools: HashSet::new(),
            switch_hints: Vec::new(),
            dvwa_objective,
            dvwa_require_findings,
            dvwa_covered: HashSet::new(),
            dvwa_validation_attempts: HashMap::new(),
            dvwa_findings: HashSet::new(),
        }
    }

    pub(super) fn coverage_instruction(&self) -> Option<String> {
        if self.dvwa_objective {
            return Some({
                format!(
                    "## DVWA 完整覆盖门槛\n本 Run 必须逐项访问并验证以下模块后才能 FINAL：{}。\n\
                 登录成功后复用 PHPSESSID/security Cookie；提交任何表单前先 GET 同一路径以获取最新 user_token，\
                 仅 GET 打开模块页面不算覆盖；每个模块必须执行带测试参数/载荷的针对性请求，\
                 弱 Session ID 必须生成至少两次并比较 Cookie。确认漏洞后立即调用 record_finding，\
                 不要依赖最终摘要补录。\n\
                 验证清单：brute 提交受控用户名/密码；exec 提交命令分隔符与无害标记；\
                 csrf/captcha 验证受保护动作；fi 使用遍历标记；upload 上传并回读无害标记文件；\
                 sqli/sqli_blind 使用 sqlmap 或差分载荷；三类 XSS 分别验证反射、持久化和 DOM sink；\
                 csp 验证允许源脚本路径；javascript 复现客户端 token。",
                    DVWA_MODULES
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join("、")
                )
            });
        }
        self.broad_web_objective.then(|| format!(
            "## 通用 Web 全量覆盖门槛\n本任务要求尽可能发现靶场漏洞，不能在单个扫描器返回后结束。\n\
             必须覆盖：{}。\n先用 hakrawler/katana + arjun/paramspider 建立端点、表单和参数地图；\
             当前目录没有所需能力时先调用 tool_search 动态检索，再对每个输入点使用专项工具；\
             nuclei 不覆盖鉴权、IDOR 和多数业务逻辑，必须使用不同身份/对象/方法的 HTTP 差分请求验证。\
             每发现一个确认问题立即 record_finding 并引用 Evidence。",
            WebCoverageArea::REQUIRED.iter().map(|area| area.label()).collect::<Vec<_>>().join("；")
        ))
    }

    pub(super) fn record_tool_arguments(
        &mut self,
        tool_name: &str,
        arguments: &str,
        success: bool,
    ) {
        if self.broad_web_objective && success {
            self.record_web_coverage(tool_name, arguments);
        }
        if !self.dvwa_objective || !success {
            return;
        }
        let normalized = arguments.to_ascii_lowercase();
        for (name, path) in DVWA_MODULES {
            if normalized.contains(path)
                && is_targeted_dvwa_validation(name, tool_name, &normalized)
            {
                let attempts = self
                    .dvwa_validation_attempts
                    .entry(name.to_string())
                    .or_default();
                *attempts += 1;
                if name == "weak-session-id" && *attempts < 2 {
                    continue;
                }
                self.dvwa_covered.insert(name.to_string());
            }
        }
    }

    fn record_web_coverage(&mut self, tool_name: &str, arguments: &str) {
        let arguments = arguments.to_ascii_lowercase();
        let area = match tool_name {
            "web_crawl" | "web_url_collect" | "js_collect" | "js_analyze" | "js_security_audit"
            | "dir_fuzz" | "ffuf" | "gobuster" | "feroxbuster" | "dirsearch" | "arjun"
            | "paramspider" | "hakrawler" => Some(WebCoverageArea::SurfaceMapping),
            "nuclei" | "web_vuln_scan" | "nikto" | "wapiti" | "afrog" | "skipfish" => {
                Some(WebCoverageArea::TemplateScan)
            }
            "sqlmap" | "sqlmap_scan" | "sqli_validate" => Some(WebCoverageArea::SqlInjection),
            "dalfox" | "web_xss_scan" | "xss_validate" | "xsser" => {
                Some(WebCoverageArea::CrossSiteScripting)
            }
            "command_injection_validate"
            | "file_inclusion_validate"
            | "file_upload_validate"
            | "commix" => Some(WebCoverageArea::ServerAndFile),
            "http_request" | "http-framework-test" | "traffic_replay"
                if looks_like_logic_validation(&arguments) =>
            {
                Some(WebCoverageArea::AccessAndBusinessLogic)
            }
            "http_request" | "http-framework-test" | "traffic_replay"
                if looks_like_server_or_file_validation(&arguments) =>
            {
                Some(WebCoverageArea::ServerAndFile)
            }
            _ => None,
        };
        if let Some(area) = area {
            self.web_coverage.insert(area);
        }
    }

    pub(super) fn record_finding_arguments(&mut self, arguments: &str, success: bool) {
        if !self.dvwa_objective || !success {
            return;
        }
        let normalized = arguments.to_ascii_lowercase();
        for (name, path) in DVWA_MODULES {
            if normalized.contains(path) {
                self.dvwa_findings.insert(name.to_string());
            }
        }
    }

    /// Restore durable progress from earlier turns in the same Run. Historical
    /// calls satisfy execution/evidence gates and seed unchanged-result
    /// detection, but do not carry a previous turn's stall streak forward.
    /// `result_fingerprint` is the semantic fingerprint persisted at record
    /// time (legacy rows fall back to the exact content hash form).
    pub(super) fn restore_tool_result(
        &mut self,
        tool_name: &str,
        success: bool,
        effective_observation: bool,
        result_fingerprint: Option<&str>,
    ) {
        self.tool_calls += 1;
        self.tools_used.insert(tool_name.to_string());
        if success && effective_observation {
            self.successful_observations += 1;
            self.evidence_categories
                .insert(ToolCategory::classify(tool_name));
        }
        if let Some(fingerprint) = result_fingerprint {
            self.seen_results.insert(fingerprint.to_string());
        }
    }

    /// Record one real or skipped tool result. Returns a model-facing
    /// diagnostic only when repeated calls/results indicate a likely stall.
    pub(super) fn record_tool_result(
        &mut self,
        tool_name: &str,
        success: bool,
        effective_observation: bool,
        content: &str,
    ) -> Option<String> {
        self.tool_calls += 1;
        self.tools_used.insert(tool_name.to_string());

        let result_fingerprint = semantic_result_fingerprint(tool_name, content);
        let new_result = self.seen_results.insert(result_fingerprint);
        let made_progress = success && effective_observation && new_result;

        if made_progress {
            self.successful_observations += 1;
            self.evidence_categories
                .insert(ToolCategory::classify(tool_name));
            self.consecutive_no_progress = 0;
            // Any successful observation clears per-tool failure tracking so
            // previously-exhausted tools become available again.
            self.consecutive_tool_failures.clear();
            self.exhausted_tools.clear();
            return None;
        }

        // A successful but fingerprint-identical result does not mean the
        // tool is broken. The shared AgentRunner still counts it as no
        // effective progress and applies its liveness guard, including when
        // resource budgets are unlimited.
        if !success {
            self.consecutive_no_progress += 1;
        }
        if !success {
            self.failed_calls += 1;
            let failures = self
                .consecutive_tool_failures
                .entry(tool_name.to_string())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            if *failures == TOOL_FAILURE_SWITCH_THRESHOLD {
                self.switch_hints
                    .push(Self::build_switch_hint(tool_name, *failures));
            }
            if *failures >= TOOL_FAILURE_SWITCH_THRESHOLD {
                self.exhausted_tools.insert(tool_name.to_string());
            }
        } else {
            // Successful but no new observation: clear failure streak for this
            // specific tool (it still works), but keep exhausted set intact.
            self.consecutive_tool_failures.remove(tool_name);
            self.exhausted_tools.remove(tool_name);
        }
        if success && !new_result {
            self.unchanged_results += 1;
        }

        if self.consecutive_no_progress == STALL_WARNING_THRESHOLD {
            return Some(self.build_stall_diagnostic());
        }
        None
    }

    /// Build a context-sensitive stall diagnostic that adapts its suggestion to
    /// what the model has already tried.
    fn build_stall_diagnostic(&self) -> String {
        let categories: Vec<&str> = self
            .evidence_categories
            .iter()
            .filter_map(|c| match c {
                ToolCategory::Recon => Some("信息收集"),
                ToolCategory::VulnValidation => Some("漏洞验证"),
                ToolCategory::Knowledge => Some("知识检索"),
                ToolCategory::EvidenceRead => Some("证据查阅"),
                ToolCategory::Unknown => None,
            })
            .collect();
        let category_hint = if categories.is_empty() {
            "尚未在任何工具类别中积累有效证据".to_string()
        } else {
            format!("已有证据类别：{}", categories.join("、"))
        };
        let suggestion = if self
            .evidence_categories
            .contains(&ToolCategory::VulnValidation)
        {
            "已有漏洞验证证据；如果该漏洞确认存在且无可继续利用的攻击面，请总结并给出 FINAL；\
             如果证据不足则使用 ASK_USER 说明还缺什么凭据"
        } else if self.evidence_categories.contains(&ToolCategory::Recon) {
            "已完成信息收集但尚未进行漏洞验证；请针对已发现端点/参数调用 dir_fuzz、\
             js_analyze 或 file_upload_validate 等验证工具，不要重复相同的信息收集请求"
        } else {
            "信息收集尚未产生有效结果；请检查目标可达性、Scope 和授权后重新尝试，\
             或使用 ASK_USER 说明当前阻塞"
        };
        format!(
            "自主循环诊断：连续 {} 次工具调用没有产生新观察。{category_hint}。\n\
             {suggestion}\n\
             不要机械重复请求；请复用已有 Evidence，改变参数或验证策略，\
             需要时使用 ASK_USER 或给出有证据的 NO_PATH。\n\
             当前统计：失败 {}，相同结果 {}，已用工具 {}/{}。",
            self.consecutive_no_progress,
            self.failed_calls,
            self.unchanged_results,
            self.tools_used.len(),
            self.tool_calls
        )
    }

    pub(super) fn stall_exhausted(&self) -> bool {
        self.consecutive_no_progress >= STALL_STOP_THRESHOLD
    }

    pub(super) fn requires_endpoint_coverage(&self) -> bool {
        self.broad_web_objective
    }

    /// Tools that have failed ≥3 consecutive times and should be temporarily
    /// removed from the model's catalog to force tool rotation.
    pub(super) fn exhausted_tools(&self) -> &HashSet<String> {
        &self.exhausted_tools
    }

    /// Drain strategy-switch hints queued since the last drain. The runtime
    /// pushes each hint into `recent_messages` once per round so the model is
    /// explicitly told to stop retrying the failing tool with similar
    /// arguments.
    pub(super) fn take_switch_hints(&mut self) -> Vec<String> {
        std::mem::take(&mut self.switch_hints)
    }

    /// Build the model-facing instruction emitted once when a tool hits the
    /// consecutive-failure switch threshold.
    fn build_switch_hint(tool_name: &str, failures: usize) -> String {
        format!(
            "运行时报告：工具 `{tool_name}` 已连续 {failures} 次调用失败。\
             停止用该工具的同类参数机械重试，立即切换策略：\n\
             - 换用其他专用工具（当前目录没有替代能力时先 tool_search 检索）；\n\
             - 或更换 payload 家族、枚举维度、请求方法等输入面；\n\
             - 若该路径确实不可行，转向其他攻击面，或在路径耗尽时给出有依据的 NO_PATH。"
        )
    }

    /// Reject a premature NO_PATH while the last call still produced new
    /// information. Once the loop has tried tools and is demonstrably stalled,
    /// NO_PATH is a valid partial outcome.
    pub(super) fn no_path_rejection(&self) -> Option<String> {
        if !self.security_objective {
            return None;
        }
        if self.tool_calls == 0 {
            return Some(
                "尚未调用任何工具，不能判定无可行路径；请先使用当前可用的低风险工具，\
                 或用 ASK_USER 明确说明缺少的授权、目标或凭据。"
                    .into(),
            );
        }
        if self.successful_observations > 0 && self.consecutive_no_progress == 0 {
            return Some(
                "上一轮刚产生新的有效观察，当前 NO_PATH 过早；请先评估该观察、形成或排除\
                 具体假设，再决定继续、询问用户或结束。"
                    .into(),
            );
        }
        // If the model has made progress but stalled for a while, NO_PATH is
        // acceptable — the model has genuinely explored and found a dead end.
        None
    }

    /// Validate a candidate FINAL response without imposing a fixed pentest
    /// phase template. The gate checks execution, evidence diversity, and
    /// evidence grounding — not headings or phase names.
    pub(super) fn final_rejection(
        &self,
        candidate: &str,
        evidence_ids: &[String],
    ) -> Option<String> {
        if !self.security_objective {
            return None;
        }
        if self.tool_calls == 0 {
            return Some(
                "安全测试目标尚未执行任何工具。请直接调用当前可用工具；若确实缺少必要输入，\
                 使用 ASK_USER；若能力或授权明确阻塞，使用 NO_PATH 并说明原因。"
                    .into(),
            );
        }
        if self.successful_observations == 0 {
            return Some(
                "所有工具调用均失败、被拦截或没有形成有效观察，不能作为成功完成。\
                 请改变策略继续验证，或使用 NO_PATH 报告工具、授权或环境阻塞。"
                    .into(),
            );
        }
        if self.dvwa_objective {
            let missing = DVWA_MODULES
                .iter()
                .filter(|(name, _)| !self.dvwa_covered.contains(*name))
                .map(|(name, path)| format!("{name}({path})"))
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Some(format!(
                    "DVWA 覆盖不完整，仍有 {} 个模块没有成功执行针对性请求：{}。\
                     请继续验证；扫描根路径或仅运行通用扫描器不能替代逐模块覆盖。",
                    missing.len(),
                    missing.join("、")
                ));
            }
            if self.dvwa_require_findings {
                let missing_findings = DVWA_MODULES
                    .iter()
                    .filter(|(name, _)| !self.dvwa_findings.contains(*name))
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>();
                if !missing_findings.is_empty() {
                    return Some(format!(
                        "DVWA security=low 预期漏洞结果不完整，仍有 {} 个模块没有形成已验证 Finding：{}。\
                         请检查认证 Cookie、载荷与响应证据并继续验证；不得用页面可达性代替漏洞结论。",
                        missing_findings.len(),
                        missing_findings.join("、")
                    ));
                }
            }
        }
        if self.broad_web_objective {
            let missing = WebCoverageArea::REQUIRED
                .iter()
                .filter(|area| !self.web_coverage.contains(area))
                .map(|area| area.label())
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Some(format!(
                    "通用 Web 覆盖不完整，仍缺少 {} 个验证域：{}。请继续执行专项工具和差分请求；不得以 nuclei 或一次根路径扫描代替。",
                    missing.len(),
                    missing.join("、")
                ));
            }
        }
        // Evidence diversity gate: require observations from at least two
        // distinct tool categories before accepting a security FINAL. A
        // single HTTP probe that returned 200 is not a penetration test.
        let productive_categories: HashSet<_> = self
            .evidence_categories
            .iter()
            .filter(|c| !matches!(c, ToolCategory::Knowledge | ToolCategory::EvidenceRead))
            .collect();
        if !self.dvwa_objective && productive_categories.len() < MIN_EVIDENCE_CATEGORY_DIVERSITY {
            let missing = if productive_categories.is_empty() {
                "信息收集和漏洞验证均未产生有效证据"
            } else if productive_categories.contains(&ToolCategory::Recon) {
                "仅完成了信息收集，需要对已发现的端点/参数执行漏洞验证工具"
            } else {
                "仅完成了漏洞验证，缺少对应的信息收集证据支撑"
            };
            return Some(format!(
                "证据多样性不足（需要至少 {MIN_EVIDENCE_CATEGORY_DIVERSITY} \
                 类工具的有效观察）。{missing}。请继续执行后再提交 FINAL。"
            ));
        }
        if has_verified_claim(candidate)
            && !evidence_ids
                .iter()
                .any(|evidence_id| candidate.contains(evidence_id))
        {
            return Some(
                "FINAL 包含“已验证/确认存在/验证成功”等事实性漏洞结论，但没有引用本 Run \
                 的 Evidence ID。请逐项引用真实 Evidence（如 ev-xxx）；没有充分证据的内容\
                 必须降级为未验证线索。"
                    .into(),
            );
        }
        None
    }

    /// Record the evidence_id → tool_name mapping so the E3 maturity gate can
    /// check cross-tool corroboration of the specific evidence cited in a FINAL.
    pub(super) fn record_evidence_tool(&mut self, evidence_id: &str, tool_name: &str) {
        self.evidence_tools
            .insert(evidence_id.to_string(), tool_name.to_string());
    }

    /// Mark that `traffic_replay` was used successfully. Reproducing a
    /// previously-captured request provides deterministic E3 corroboration
    /// because it proves the finding is reproducible rather than ephemeral.
    pub(super) fn record_traffic_replay(&mut self) {
        self.traffic_replay_count += 1;
    }

    /// Evidence maturity gate for verified claims in a candidate FINAL.
    /// Activated when `PENTA_MIN_EVIDENCE_MATURITY` is set to E3 or higher.
    ///
    /// E3 requires the evidence cited by verified claims to come from ≥2
    /// distinct tool categories (recon + validation), or to include a
    /// successful `traffic_replay` which provides independent corroboration.
    /// E4 cannot be enforced at FINAL time (requires human confirmation) and
    /// is treated as E3 for gate purposes.
    pub(super) fn maturity_rejection(
        &self,
        candidate: &str,
        evidence_ids: &[String],
    ) -> Option<String> {
        self.maturity_rejection_with_level(candidate, evidence_ids, None)
    }

    /// Same as [`maturity_rejection`] but allows overriding the minimum
    /// maturity level (used by tests; production code passes `None`).
    fn maturity_rejection_with_level(
        &self,
        candidate: &str,
        evidence_ids: &[String],
        override_level: Option<u8>,
    ) -> Option<String> {
        let required = override_level.unwrap_or_else(min_evidence_maturity);
        if required < 3 {
            return None;
        }
        if !has_verified_claim(candidate) {
            return None;
        }
        // Find which evidence IDs the candidate actually cites
        let cited: Vec<&String> = evidence_ids
            .iter()
            .filter(|id| candidate.contains(id.as_str()))
            .collect();
        if cited.is_empty() {
            // Citation gate in final_rejection already handles this case
            return None;
        }
        // Count distinct maturity categories among cited evidence
        let mut categories: HashSet<&str> = HashSet::new();
        for id in &cited {
            if let Some(tool) = self.evidence_tools.get(id.as_str()) {
                if let Some(cat) = ToolCategory::classify(tool).maturity_category() {
                    categories.insert(cat);
                }
            }
        }
        // traffic_replay provides E3-level corroboration
        let effective = categories.len() + (self.traffic_replay_count > 0) as usize;
        if effective < 2 {
            return Some(format!(
                "证据成熟度不足：当前要求 E{required}（多工具类别交叉印证），\
                 但已验证结论仅引用 {} 类工具证据（{}）。\
                 请使用不同类别的工具交叉验证同一发现，或使用 traffic_replay 复现关键请求。\
                 当前累积：recon 工具 {} 次，validation 工具 {} 次，traffic_replay {} 次。",
                categories.len(),
                if categories.is_empty() {
                    "无有效类别".to_string()
                } else {
                    categories.iter().copied().collect::<Vec<_>>().join("、")
                },
                self.evidence_categories
                    .iter()
                    .filter(|c| matches!(c, ToolCategory::Recon))
                    .count(),
                self.evidence_categories
                    .iter()
                    .filter(|c| matches!(c, ToolCategory::VulnValidation))
                    .count(),
                self.traffic_replay_count,
            ));
        }
        None
    }

    #[cfg(test)]
    fn consecutive_no_progress(&self) -> usize {
        self.consecutive_no_progress
    }
}

/// Semantic result fingerprint: normalize volatile tokens (timestamps, UUIDs,
/// nonces, long hex/base64 secrets, epoch milliseconds) before hashing so
/// responses that differ only in per-request noise still deduplicate across
/// turns. Deterministic — no model involvement.
pub(super) fn semantic_result_fingerprint(tool_name: &str, content: &str) -> String {
    let normalized = match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value) => normalize_volatile_value(value).to_string(),
        Err(_) => normalize_volatile_text(content),
    };
    format!(
        "{tool_name}:semantic:{:x}",
        Sha256::digest(normalized.as_bytes())
    )
}

fn normalize_volatile_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(normalize_volatile_text(&text))
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(normalize_volatile_value).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, normalize_volatile_value(item)))
                .collect(),
        ),
        other => other,
    }
}

fn normalize_volatile_text(text: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static VOLATILE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        vec![
            // ISO-8601 / common log timestamps first (longest patterns win).
            (
                Regex::new(
                    r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(:\d{2})?(\.\d+)?(Z|[+-]\d{2}:?\d{2})?",
                )
                .unwrap(),
                "<ts>",
            ),
            // UUIDs.
            (
                Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                    .unwrap(),
                "<uuid>",
            ),
            // Epoch milliseconds / seconds with fractional part.
            (Regex::new(r"\b1\d{12}\b").unwrap(), "<epoch>"),
            // Long hex or base64-ish tokens (nonces, CSRF, session ids).
            (
                Regex::new(r"(?i)\b[0-9a-z+/=_-]{24,}\b").unwrap(),
                "<token>",
            ),
        ]
    });
    let mut result = text.to_string();
    for (pattern, replacement) in VOLATILE.iter() {
        result = pattern.replace_all(&result, *replacement).into_owned();
    }
    result
}

pub(super) fn has_verified_claim(content: &str) -> bool {
    let normalized = content.to_ascii_lowercase();
    if [
        "漏洞已验证",
        "验证成功",
        "确认存在",
        "成功利用",
        "verified vulnerability",
        "confirmed vulnerability",
        "exploitation succeeded",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
    {
        return true;
    }
    section_has_content(content, "已验证漏洞") || section_has_content(content, "已确认漏洞")
}

fn section_has_content(content: &str, heading: &str) -> bool {
    let mut in_section = false;
    let mut body = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('#') {
            if in_section {
                break;
            }
            if line.contains(heading) {
                in_section = true;
            }
            continue;
        }
        if in_section && !line.is_empty() {
            body.push(line);
        }
    }
    !body.is_empty()
        && !body.iter().all(|line| {
            matches!(
                line.trim_matches(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '-' | '*' | '。' | '.' | ':' | '：')
                }),
                "无" | "暂无" | "未发现" | "无已验证漏洞"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{DVWA_MODULES, ScanProgress};

    #[test]
    fn ordinary_chat_does_not_require_tools_or_evidence() {
        let progress = ScanProgress::new("你好");
        assert!(progress.final_rejection("FINAL: 你好", &[]).is_none());
    }

    #[test]
    fn security_final_requires_real_tool_progress() {
        let progress = ScanProgress::new("扫描 https://example.test");
        assert!(
            progress
                .final_rejection("FINAL: 未发现问题", &[])
                .unwrap()
                .contains("尚未执行")
        );
    }

    #[test]
    fn dvwa_final_requires_each_vulnerable_module() {
        let mut progress = ScanProgress::new("对 DVWA http://dvwa 做完整渗透测试");
        progress.record_tool_result("http_request", true, true, r#"{"status":200}"#);
        progress.record_tool_arguments(
            "http_request",
            r#"{"url":"http://dvwa/vulnerabilities/sqli/?id=1%27%20or%20%271%27=%271"}"#,
            true,
        );
        let rejection = progress
            .final_rejection("FINAL: 阶段性完成", &["ev-1".into()])
            .expect("incomplete DVWA coverage must be rejected");
        assert!(rejection.contains("DVWA 覆盖不完整"));
        assert!(rejection.contains("file-upload"));
    }

    #[test]
    fn opening_dvwa_module_pages_does_not_count_as_validation() {
        let mut progress = ScanProgress::new("完整测试 DVWA http://dvwa");
        for (_, path) in DVWA_MODULES {
            progress.record_tool_arguments(
                "http_request",
                &format!(r#"{{"url":"http://dvwa{path}","method":"GET"}}"#),
                true,
            );
        }
        assert!(progress.dvwa_covered.is_empty());
    }

    #[test]
    fn weak_session_requires_two_successful_generations() {
        let mut progress = ScanProgress::new("完整测试 DVWA http://dvwa");
        let arguments = r#"{"url":"http://dvwa/vulnerabilities/weak_id/","method":"POST"}"#;
        progress.record_tool_arguments("http_request", arguments, true);
        assert!(!progress.dvwa_covered.contains("weak-session-id"));
        progress.record_tool_arguments("http_request", arguments, true);
        assert!(progress.dvwa_covered.contains("weak-session-id"));
    }

    #[test]
    fn low_security_dvwa_requires_a_finding_for_every_module() {
        let mut progress = ScanProgress::new("完整测试 DVWA http://dvwa security=low");
        progress.record_tool_result("http_request", true, true, r#"{"status":200}"#);
        progress.dvwa_covered = DVWA_MODULES
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        let rejection = progress
            .final_rejection("FINAL: 完成", &["ev-1".into()])
            .expect("low security DVWA must produce expected findings");
        assert!(rejection.contains("没有形成已验证 Finding"));

        for (_, path) in DVWA_MODULES {
            progress
                .record_finding_arguments(&format!(r#"{{"target":"http://dvwa{path}"}}"#), true);
        }
        assert!(
            progress
                .final_rejection("FINAL: 完成", &["ev-1".into()])
                .is_none()
        );
    }

    #[test]
    fn single_tool_with_valid_observation_does_not_complete_security_test() {
        let mut progress = ScanProgress::new("只测试SQL https://example.test");
        progress.record_tool_result("http_request", true, true, r#"{"status":200,"body":"ok"}"#);
        assert!(
            progress
                .final_rejection("FINAL: 目标可达，未发现明显漏洞", &["ev-1".into()])
                .is_some()
        );
    }

    #[test]
    fn nmap_plus_nuclei_do_not_satisfy_broad_web_coverage() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_tool_result(
            "nmap",
            true,
            true,
            r#"{"ports":[{"port":80,"state":"open"}]}"#,
        );
        progress.record_tool_result(
            "nuclei",
            true,
            true,
            r#"{"findings":[{"template":"exposed-panel"}]}"#,
        );
        assert!(
            progress
                .final_rejection("FINAL: 发现开放端口，Nuclei 未命中高危", &[])
                .is_some()
        );
    }

    #[test]
    fn verified_claim_requires_known_evidence_reference() {
        let mut progress = ScanProgress::new("只测试SQL https://example.test");
        progress.record_tool_result(
            "http_request",
            true,
            true,
            r#"{"status":200,"marker":"proof"}"#,
        );
        progress.record_tool_result(
            "dir_fuzz",
            true,
            true,
            r#"{"findings":[{"path":"/admin"}]}"#,
        );
        progress.record_tool_result(
            "nmap",
            true,
            true,
            r#"{"ports":[{"port":80,"state":"open"}]}"#,
        );
        let evidence = vec!["ev-123".to_string()];
        assert!(
            progress
                .final_rejection("FINAL: 漏洞已验证", &evidence)
                .unwrap()
                .contains("Evidence ID")
        );
        assert!(
            progress
                .final_rejection("FINAL: 漏洞已验证，证据 ev-123", &evidence)
                .is_none()
        );
    }

    #[test]
    fn semantic_fingerprint_ignores_volatile_noise() {
        use super::semantic_result_fingerprint;
        let a = r#"{"status":200,"body":"token=3f2504e0-4f89-11d3-9a0c-0305e82c3301 ts=2026-07-28T10:00:00Z ok"}"#;
        let b = r#"{"status":200,"body":"token=7c9e6679-7425-40de-944b-e07fc1f90ae7 ts=2026-07-28T11:30:05Z ok"}"#;
        assert_eq!(
            semantic_result_fingerprint("http_request", a),
            semantic_result_fingerprint("http_request", b),
            "仅 nonce/时间戳不同的响应应得到相同语义指纹"
        );
        let c = r#"{"status":200,"body":"different content entirely"}"#;
        assert_ne!(
            semantic_result_fingerprint("http_request", a),
            semantic_result_fingerprint("http_request", c)
        );
    }

    #[test]
    fn restored_semantic_fingerprint_blocks_cross_turn_repeat() {
        use super::semantic_result_fingerprint;
        let mut progress = ScanProgress::new("扫描 https://example.test");
        // Simulate a previous turn's successful call restored via fingerprint.
        progress.restore_tool_result(
            "http_request",
            true,
            true,
            Some(&semantic_result_fingerprint(
                "http_request",
                r#"{"status":200,"body":"nonce abcdefghijklmnopqrstuvwxyz12 page"}"#,
            )),
        );
        // Same page with a different nonce: a successful repeat does NOT
        // increment the stall counter (tools work, model must change strategy).
        let diagnostic = progress.record_tool_result(
            "http_request",
            true,
            true,
            r#"{"status":200,"body":"nonce zyxwvutsrqponmlkjihgfedcba987654 page"}"#,
        );
        assert!(diagnostic.is_none(), "successful repeat is not a stall");
        assert_eq!(progress.consecutive_no_progress(), 0);
    }

    #[test]
    fn repeated_failures_trigger_stall_diagnostic() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        assert!(
            progress
                .record_tool_result("http_request", true, true, r#"{"status":200}"#)
                .is_none()
        );
        // Successful repeated results don't increment the stall counter.
        for _ in 0..2 {
            progress.record_tool_result("http_request", true, true, r#"{"status":200}"#);
        }
        assert_eq!(
            progress.consecutive_no_progress(),
            0,
            "successful repeats are not stall"
        );
        // Failed calls DO increment the counter — that's the real stall signal.
        let mut diagnostic = None;
        for _ in 0..3 {
            if let Some(d) = progress.record_tool_result(
                "http_probe",
                false,
                false,
                r#"{"error":"network unreachable"}"#,
            ) {
                diagnostic = Some(d);
            }
        }
        let diagnostic = diagnostic.expect("3rd consecutive failure must emit diagnostic");
        assert!(diagnostic.contains("没有产生新观察"));
        assert!(diagnostic.contains("已用工具"));
        assert_eq!(progress.consecutive_no_progress(), 3);
    }

    #[test]
    fn third_same_tool_failure_emits_one_switch_hint_and_success_resets_it() {
        let mut progress = ScanProgress::new("扫描 https://example.test");

        for failure in 1..=2 {
            progress.record_tool_result(
                "sqlmap",
                false,
                false,
                &format!(r#"{{"error":"failed-{failure}"}}"#),
            );
            assert!(progress.take_switch_hints().is_empty());
            assert!(!progress.exhausted_tools().contains("sqlmap"));
        }

        progress.record_tool_result("sqlmap", false, false, r#"{"error":"failed-3"}"#);
        let hints = progress.take_switch_hints();
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("连续 3 次"));
        assert!(hints[0].contains("切换策略"));
        assert!(progress.take_switch_hints().is_empty());
        assert!(progress.exhausted_tools().contains("sqlmap"));

        progress.record_tool_result(
            "http_request",
            true,
            true,
            r#"{"status":200,"marker":"fresh-evidence"}"#,
        );
        assert!(progress.exhausted_tools().is_empty());

        progress.record_tool_result("sqlmap", false, false, r#"{"error":"new-streak"}"#);
        assert!(progress.take_switch_hints().is_empty());
        assert!(!progress.exhausted_tools().contains("sqlmap"));
    }

    #[test]
    fn stall_diagnostic_suggests_validation_after_recon() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        // First call produces recon evidence
        progress.record_tool_result(
            "http_request",
            true,
            true,
            r#"{"status":200,"body":"<form action='/login'>"}"#,
        );
        // Failed vuln-validation tools push the counter to the warning threshold.
        let mut diagnostic = None;
        for _ in 0..3 {
            if let Some(d) = progress.record_tool_result(
                "sqli_validate",
                false,
                false,
                r#"{"error":"no injection point configured"}"#,
            ) {
                diagnostic = Some(d);
            }
        }
        let diagnostic = diagnostic.expect("3rd consecutive failure must emit diagnostic");
        assert!(
            diagnostic.contains("漏洞验证"),
            "should suggest vuln validation after recon failures, got: {diagnostic}"
        );
    }

    #[test]
    fn no_path_is_rejected_immediately_after_new_evidence() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_tool_result(
            "http_request",
            true,
            true,
            r#"{"status":200,"marker":"new"}"#,
        );
        assert!(progress.no_path_rejection().is_some());
        progress.record_tool_result("http_request", false, false, r#"{"error":"timeout"}"#);
        assert!(progress.no_path_rejection().is_none());
    }

    #[test]
    fn tool_categories_are_tracked_for_diversity() {
        let mut progress = ScanProgress::new("只测XSS https://example.test");
        progress.record_tool_result("http_request", true, true, r#"{"status":200}"#);
        progress.record_tool_result("dir_fuzz", true, true, r#"{"findings":[]}"#);
        progress.record_tool_result(
            "nmap",
            true,
            true,
            r#"{"ports":[{"port":80,"state":"open"}]}"#,
        );
        assert!(
            progress
                .final_rejection("FINAL: no vulns found", &[])
                .is_none()
        );
    }

    #[test]
    fn broad_web_scan_rejects_drive_by_nuclei_result() {
        let mut progress = ScanProgress::new("全面渗透测试 https://example.test");
        progress.record_tool_arguments("nuclei", r#"{"target":"https://example.test"}"#, true);
        progress.record_tool_result("nuclei", true, true, r#"{"matched":1}"#);
        let rejection = progress
            .final_rejection("FINAL: nuclei found one issue", &[])
            .expect("single scanner must not complete a broad assessment");
        assert!(rejection.contains("覆盖不完整"));
        assert!(rejection.contains("SQL 注入"));
        assert!(rejection.contains("鉴权"));
    }

    #[test]
    fn broad_web_scan_accepts_all_required_validation_domains() {
        let mut progress = ScanProgress::new("全面渗透测试 https://example.test");
        let calls = [
            ("web_crawl", r#"{"target":"https://example.test"}"#),
            ("nuclei", r#"{"target":"https://example.test"}"#),
            ("sqlmap_scan", r#"{"url":"https://example.test/?id=1"}"#),
            ("web_xss_scan", r#"{"url":"https://example.test/?q=x"}"#),
            (
                "file_inclusion_validate",
                r#"{"url":"https://example.test/files?name=../x"}"#,
            ),
            (
                "http_request",
                r#"{"url":"https://example.test/users/2","method":"POST","headers":{"Cookie":"session=a"}}"#,
            ),
        ];
        for (index, (tool, arguments)) in calls.into_iter().enumerate() {
            progress.record_tool_arguments(tool, arguments, true);
            progress.record_tool_result(tool, true, true, &format!(r#"{{"result":{index}}}"#));
        }
        assert!(
            progress
                .final_rejection("FINAL: assessment complete", &[])
                .is_none()
        );
    }

    // ── Evidence maturity gate tests ───────────────────────────────────

    #[test]
    fn maturity_gate_is_inactive_by_default() {
        // Default is E2, so the maturity gate should not reject.
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_tool_result("http_request", true, true, r#"{"status":200,"body":"ok"}"#);
        progress.record_evidence_tool("ev-1", "http_request");
        let rejection = progress.maturity_rejection_with_level(
            "FINAL: 漏洞已验证，参考 ev-1",
            &["ev-1".to_string()],
            None, // use env default (E2)
        );
        assert!(
            rejection.is_none(),
            "E2 default should not reject: {rejection:?}"
        );
    }

    #[test]
    fn maturity_e3_accepts_cross_tool_corroboration() {
        // Simulate evidence from two categories cited by verified claims.
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_evidence_tool("ev-1", "http_request");
        progress.record_evidence_tool("ev-2", "dir_fuzz");
        let rejection = progress.maturity_rejection_with_level(
            "FINAL: 漏洞已验证，见 ev-1 和 ev-2",
            &["ev-1".to_string(), "ev-2".to_string()],
            Some(3), // E3
        );
        assert!(
            rejection.is_none(),
            "E3 with 2 categories should pass: {rejection:?}"
        );
    }

    #[test]
    fn maturity_e3_rejects_single_category() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_evidence_tool("ev-1", "http_request");
        progress.record_evidence_tool("ev-2", "http_probe");
        let rejection = progress.maturity_rejection_with_level(
            "FINAL: 漏洞已验证，见 ev-1 和 ev-2",
            &["ev-1".to_string(), "ev-2".to_string()],
            Some(3), // E3
        );
        assert!(rejection.is_some(), "E3 with single category should reject");
        let msg = rejection.unwrap();
        assert!(msg.contains("成熟度不足"), "should mention maturity: {msg}");
    }

    #[test]
    fn maturity_e3_passes_with_traffic_replay() {
        // traffic_replay provides E3 corroboration as a second category
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_evidence_tool("ev-1", "http_request");
        progress.record_traffic_replay();
        let rejection = progress.maturity_rejection_with_level(
            "FINAL: 漏洞已验证，已通过 traffic_replay 复现，见 ev-1",
            &["ev-1".to_string()],
            Some(3), // E3
        );
        assert!(
            rejection.is_none(),
            "E3 with recon + replay should pass: {rejection:?}"
        );
    }

    #[test]
    fn maturity_gate_skips_non_verified_claims() {
        // No verified claim → gate is skipped entirely
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_evidence_tool("ev-1", "http_request");
        let rejection = progress.maturity_rejection_with_level(
            "FINAL: 未发现高危漏洞，目标安全配置良好",
            &["ev-1".to_string()],
            Some(3), // E3
        );
        assert!(
            rejection.is_none(),
            "non-verified claims should skip maturity gate: {rejection:?}"
        );
    }

    #[test]
    fn maturity_gate_skips_when_no_evidence_cited() {
        // Cited evidence empty → citation gate handles this case
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_evidence_tool("ev-1", "http_request");
        let rejection = progress.maturity_rejection_with_level(
            "FINAL: 漏洞已验证",
            &["ev-1".to_string()], // ev-1 in evidence_ids but NOT cited in text
            Some(3),               // E3
        );
        assert!(
            rejection.is_none(),
            "no cited evidence should delegate to citation gate: {rejection:?}"
        );
    }
}
