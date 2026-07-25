use std::collections::HashSet;

const MAX_GATE_CONTINUATIONS: usize = 4;
const MIN_VALIDATION_ATTEMPTS_PER_UNVERIFIED_ITEM: usize = 2;

pub(super) struct ScanProgress {
    scan_objective: bool,
    attempted_tools: HashSet<String>,
    continuations: usize,
    knowledge_searches: usize,
    validation_attempts_after_knowledge: usize,
}

impl ScanProgress {
    pub(super) fn new(user_input: &str) -> Self {
        let normalized = user_input.to_ascii_lowercase();
        let scan_objective = [
            "http://",
            "https://",
            "扫描",
            "渗透",
            "漏洞",
            "安全测试",
            "分析js",
            "分析 js",
        ]
        .iter()
        .any(|pattern| normalized.contains(pattern));
        Self {
            scan_objective,
            attempted_tools: HashSet::new(),
            continuations: 0,
            knowledge_searches: 0,
            validation_attempts_after_knowledge: 0,
        }
    }

    pub(super) fn record_tool(&mut self, tool_name: &str) {
        self.attempted_tools.insert(tool_name.to_string());
    }

    pub(super) fn record_tool_result(
        &mut self,
        tool_name: &str,
        success: bool,
        duplicate_skipped: bool,
    ) {
        if duplicate_skipped {
            return;
        }
        if tool_name == "knowledge_search" && success {
            self.knowledge_searches += 1;
        } else if tool_name != "knowledge_search"
            && self.knowledge_searches > 0
            && self.validation_attempts_after_knowledge
                < self.knowledge_searches * MIN_VALIDATION_ATTEMPTS_PER_UNVERIFIED_ITEM
        {
            self.validation_attempts_after_knowledge += 1;
        }
    }

    pub(super) fn continuation_prompt(&mut self, candidate: &str) -> Option<String> {
        if !self.scan_objective {
            return None;
        }
        let missing = self.missing_stages(candidate);
        if missing.is_empty() {
            return None;
        }
        let knowledge_closure_required = missing
            .iter()
            .any(|stage| stage.contains("Wiki") || stage.contains("知识"));
        if self.continuations >= MAX_GATE_CONTINUATIONS && !knowledge_closure_required {
            return None;
        }
        self.continuations += 1;
        let unverified = unverified_items(candidate);
        let unverified_instruction = if unverified.is_empty() {
            String::new()
        } else {
            format!(
                "\n未验证线索共 {} 项：{}。\n\
                 每项至少检索一次对应 Wiki，并执行至少 {} 次非重复的实际工具验证；\
                 严重度低不能减少验证次数。仍无法确认时只能标记为“可能存在”，并逐项写明\
                 已尝试方法、关联 Evidence、无法确认原因和下一步人工/扩展工具复核思路。",
                unverified.len(),
                unverified.join("；"),
                MIN_VALIDATION_ATTEMPTS_PER_UNVERIFIED_ITEM
            )
        };
        Some(format!(
            "系统执行门禁：本轮还不能结束，仅完成存活探测不算完成扫描。\n\
             尚未覆盖：{}。\n\
             请继续调用当前可用工具完成缺失阶段。优先进行受控目录发现、JS 收集与逐文件分析、\
             对发现端点进行非破坏性 HTTP 验证。只有在完成这些阶段，或明确记录某阶段为何被\
             授权/能力边界阻塞后，才能输出最终报告。\n\
             最终报告必须一次性完整输出，并分别包含标题级章节“已验证漏洞”“未验证线索”\
             “安全配置问题”和“修复建议”；某类为空也必须明确写“无”。每项结论引用 Evidence，\
             不要把原始工具响应拼入报告。{}",
            missing.join("、"),
            unverified_instruction
        ))
    }

    fn missing_stages(&self, candidate: &str) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.attempted("web_crawl") {
            missing.push("页面结构爬取");
        }
        if !self.attempted("dir_fuzz") {
            missing.push("受控目录与文件发现");
        }
        if !self.attempted("js_collect") {
            missing.push("JavaScript 文件收集");
        }
        if !self.attempted("js_analyze") {
            missing.push("JavaScript/API/鉴权逻辑分析");
        }
        if !self.attempted("http_request") {
            missing.push("发现端点的 HTTP 验证");
        }
        if !looks_like_report(candidate) {
            missing.push("最终安全测试报告");
        }
        let unverified_count = unverified_items(candidate).len();
        if unverified_count > 0 {
            if self.knowledge_searches < unverified_count {
                missing.push("针对每条未验证线索逐项检索 Wiki 方法论");
            } else if self.validation_attempts_after_knowledge
                < unverified_count * MIN_VALIDATION_ATTEMPTS_PER_UNVERIFIED_ITEM
            {
                missing.push("依据 Wiki 对每条线索执行多次非重复工具验证");
            }
        }
        missing
    }

    fn attempted(&self, tool_name: &str) -> bool {
        self.attempted_tools.contains(tool_name)
    }
}

fn looks_like_report(content: &str) -> bool {
    let content = content.trim();
    content.contains("报告")
        && has_heading(content, "已验证")
        && has_heading(content, "未验证")
        && has_heading(content, "配置")
        && has_heading(content, "修复")
}

fn has_heading(content: &str, keyword: &str) -> bool {
    content
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with('#') && line.contains(keyword))
}

fn has_nonempty_unverified_section(content: &str) -> bool {
    let mut in_section = false;
    let mut body = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('#') {
            if in_section {
                break;
            }
            if line.contains("未验证") {
                in_section = true;
            }
            continue;
        }
        if in_section && !line.is_empty() {
            body.push(line);
        }
    }
    if body.is_empty() {
        return false;
    }
    !body.iter().all(|line| {
        matches!(
            line.trim_matches(|character: char| {
                character.is_whitespace()
                    || matches!(character, '-' | '*' | '。' | '.' | ':' | '：')
            }),
            "无" | "暂无" | "无未验证线索"
        )
    })
}

fn unverified_items(content: &str) -> Vec<String> {
    if !has_nonempty_unverified_section(content) {
        return Vec::new();
    }
    let mut in_section = false;
    let mut items = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('#') {
            if in_section {
                break;
            }
            if line.contains("未验证") {
                in_section = true;
            }
            continue;
        }
        if !in_section || line.is_empty() {
            continue;
        }
        let label = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
            .or_else(|| {
                let digits = line.chars().take_while(char::is_ascii_digit).count();
                if digits == 0 {
                    return None;
                }
                line.get(digits..).and_then(|rest| {
                    rest.strip_prefix(". ")
                        .or_else(|| rest.strip_prefix("、"))
                        .or_else(|| rest.strip_prefix(") "))
                })
            });
        if let Some(label) = label {
            let label = label.trim().trim_matches('*').trim();
            if !label.is_empty() && !items.iter().any(|item| item == label) {
                items.push(label.chars().take(160).collect());
            }
        }
    }
    if items.is_empty() {
        vec!["未验证线索".to_string()]
    } else {
        items
    }
}

#[cfg(test)]
mod tests {
    use super::ScanProgress;

    #[test]
    fn ordinary_chat_is_not_forced_through_scan_gate() {
        let mut progress = ScanProgress::new("你好");
        assert!(
            progress
                .continuation_prompt("你好，有什么可以帮助？")
                .is_none()
        );
    }

    #[test]
    fn probe_only_scan_must_continue() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        progress.record_tool("http_probe");
        let prompt = progress.continuation_prompt("目标存活").unwrap();
        assert!(prompt.contains("目录"));
        assert!(prompt.contains("JavaScript"));
    }

    #[test]
    fn covered_scan_with_report_can_finish() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        for tool in [
            "web_crawl",
            "dir_fuzz",
            "js_collect",
            "js_analyze",
            "http_request",
        ] {
            progress.record_tool(tool);
        }
        assert!(
            progress
                .continuation_prompt(
                    "# 安全测试报告\n## 已验证漏洞\n无\n## 未验证线索\n无\n\
                     ## 安全配置问题\n无\n## 修复建议\n无"
                )
                .is_none()
        );
    }

    #[test]
    fn report_summary_claims_do_not_replace_required_sections() {
        let mut progress = ScanProgress::new("扫描 https://example.test");
        for tool in [
            "web_crawl",
            "dir_fuzz",
            "js_collect",
            "js_analyze",
            "http_request",
        ] {
            progress.record_tool(tool);
        }
        assert!(
            progress
                .continuation_prompt(
                    "# 安全测试报告\n共发现 9 个已验证漏洞、5 个未验证线索和 3 个配置问题"
                )
                .is_some()
        );
    }

    #[test]
    fn unverified_findings_require_wiki_then_validation() {
        let report = "# 安全测试报告\n## 已验证漏洞\n无\n## 未验证线索\n\
                      - CSP 绕过尚未验证\n## 安全配置问题\n无\n## 修复建议\n复核";
        let mut progress = ScanProgress::new("扫描 https://example.test");
        for tool in [
            "web_crawl",
            "dir_fuzz",
            "js_collect",
            "js_analyze",
            "http_request",
        ] {
            progress.record_tool(tool);
        }
        assert!(
            progress
                .continuation_prompt(report)
                .unwrap()
                .contains("Wiki")
        );
        progress.record_tool_result("knowledge_search", true, false);
        assert!(
            progress
                .continuation_prompt(report)
                .unwrap()
                .contains("多次非重复工具验证")
        );
        progress.record_tool_result("http_request", true, false);
        assert!(progress.continuation_prompt(report).is_some());
        progress.record_tool_result("http_request", false, false);
        assert!(progress.continuation_prompt(report).is_none());
    }

    #[test]
    fn every_unverified_item_requires_knowledge_and_multiple_attempts() {
        let report = "# 安全测试报告\n## 已验证漏洞\n无\n## 未验证线索\n\
                      1. CSP 绕过可能存在\n2. 弱会话标识可能存在\n\
                      ## 安全配置问题\n无\n## 修复建议\n复核";
        let mut progress = ScanProgress::new("扫描 https://example.test");
        for tool in [
            "web_crawl",
            "dir_fuzz",
            "js_collect",
            "js_analyze",
            "http_request",
        ] {
            progress.record_tool(tool);
        }
        progress.record_tool_result("knowledge_search", true, false);
        progress.record_tool_result("http_request", true, false);
        progress.record_tool_result("http_request", true, true);
        assert!(progress.continuation_prompt(report).is_some());

        progress.record_tool_result("http_request", false, false);
        progress.record_tool_result("knowledge_search", true, false);
        progress.record_tool_result("http_request", true, false);
        assert!(progress.continuation_prompt(report).is_some());
        progress.record_tool_result("http_request", false, false);
        assert!(progress.continuation_prompt(report).is_none());
    }
}
