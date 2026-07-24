use std::collections::HashSet;

const MAX_GATE_CONTINUATIONS: usize = 4;

pub(super) struct ScanProgress {
    scan_objective: bool,
    attempted_tools: HashSet<String>,
    continuations: usize,
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
        }
    }

    pub(super) fn record_tool(&mut self, tool_name: &str) {
        self.attempted_tools.insert(tool_name.to_string());
    }

    pub(super) fn continuation_prompt(&mut self, candidate: &str) -> Option<String> {
        if !self.scan_objective || self.continuations >= MAX_GATE_CONTINUATIONS {
            return None;
        }
        let missing = self.missing_stages(candidate);
        if missing.is_empty() {
            return None;
        }
        self.continuations += 1;
        Some(format!(
            "系统执行门禁：本轮还不能结束，仅完成存活探测不算完成扫描。\n\
             尚未覆盖：{}。\n\
             请继续调用当前可用工具完成缺失阶段。优先进行受控目录发现、JS 收集与逐文件分析、\
             对发现端点进行非破坏性 HTTP 验证。只有在完成这些阶段，或明确记录某阶段为何被\
             授权/能力边界阻塞后，才能输出包含证据、已验证发现、未验证线索和修复建议的最终报告。",
            missing.join("、")
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
        missing
    }

    fn attempted(&self, tool_name: &str) -> bool {
        self.attempted_tools.contains(tool_name)
    }
}

fn looks_like_report(content: &str) -> bool {
    let content = content.trim();
    content.contains("报告")
        && ["发现", "证据", "结论", "修复"]
            .iter()
            .any(|section| content.contains(section))
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
                .continuation_prompt("安全测试报告：发现与证据如下，结论及修复建议如下。")
                .is_none()
        );
    }
}
