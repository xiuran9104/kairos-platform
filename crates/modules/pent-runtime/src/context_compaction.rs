//! Context compaction policy and immutable user-intent ledger for agent runtimes.

use serde_json::{Value, json};

/// 渗透领域定制的历史上下文摘要提示词：压缩旧消息时必须逐节输出
/// 下列类目，无内容的类目明确写“无”，不得省略也不得虚构。
pub const SUMMARY_SYSTEM_PROMPT: &str = r#"你是渗透测试 Agent 的历史上下文压缩器。将旧消息压缩为结构化摘要，必须逐节输出以下类目（该类目无内容时明确写“无”）：
1. 已确认漏洞及攻击链：漏洞名、目标、支撑 Evidence ID、利用路径；
2. 已获取凭证/令牌：来源、类型、可用范围；
3. 已验证有效的 payload：完整 payload 原文、目标端点、触发效果；
4. 已排除路径与失败死路：失败动作与原因，避免后续重复尝试；
5. 资产/端点枚举结论：主机、端口、服务、目录、API 等已枚举资产；
6. 当前计划进度：已完成步骤、未完成步骤、下一步；
7. 目标、授权范围与用户约束。
Evidence ID、有效 payload、令牌标识和具体端点必须原样保留，不得虚构。只输出摘要。"#;

/// fallback_summary 每个类目最多保留的条目数。
const FALLBACK_CATEGORY_LIMIT: usize = 8;
/// fallback_summary 单条摘录的最大字符数。
const FALLBACK_EXCERPT_CHARS: usize = 300;

pub struct CompactionInput {
    pub older: Vec<Value>,
    pub retained: Vec<Value>,
    pub user_ledger: String,
}

pub fn prepare(messages: &[Value], budget: u64) -> Option<CompactionInput> {
    let estimated = messages.iter().map(estimate).sum::<u64>();
    if estimated < budget.saturating_mul(8) / 10 || messages.len() <= 14 {
        return None;
    }
    let split = messages.len().saturating_sub(12).max(1);
    let older = messages[..split].to_vec();
    let retained = messages[split..].to_vec();
    let ledger = messages
        .iter()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str())
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.chars().take(16_000).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n---\n");
    Some(CompactionInput {
        older,
        retained,
        user_ledger: ledger.chars().take(96_000).collect(),
    })
}

pub fn merge(summary: String, input: CompactionInput) -> Vec<Value> {
    let mut output = vec![
        json!({"role":"system","content":format!("## 历史上下文压缩摘要\n{summary}\n\n## 用户原始意图账本（不可被摘要覆盖）\n{}", input.user_ledger)}),
    ];
    output.extend(input.retained);
    output
}

pub fn fallback_summary(messages: &[Value]) -> String {
    const CATEGORIES: [(&str, &[&str]); 7] = [
        (
            "已确认漏洞及攻击链",
            &[
                "已确认",
                "已验证漏洞",
                "confirmed",
                "verified vulnerability",
                "evidence",
            ],
        ),
        (
            "已获取凭证/令牌",
            &[
                "credential",
                "token",
                "cookie",
                "authorization",
                "bearer",
                "jwt",
                "密码",
                "令牌",
                "凭证",
            ],
        ),
        (
            "已验证有效的 payload",
            &[
                "payload",
                "union select",
                "<script",
                "../",
                "curl ",
                "请求体",
                "触发效果",
            ],
        ),
        (
            "已排除路径与失败死路",
            &[
                "failed",
                "failure",
                "error",
                "timeout",
                "404",
                "no_path",
                "失败",
                "超时",
                "未命中",
                "不可达",
            ],
        ),
        (
            "资产/端点枚举结论",
            &[
                "http://",
                "https://",
                "endpoint",
                "host",
                "port",
                "service",
                "directory",
                "api",
                "端点",
                "主机",
                "端口",
                "目录",
            ],
        ),
        (
            "当前计划进度",
            &[
                "plan",
                "step",
                "next",
                "todo",
                "计划",
                "步骤",
                "下一步",
                "已完成",
                "未完成",
            ],
        ),
        (
            "目标、授权范围与用户约束",
            &[
                "target",
                "scope",
                "authorization",
                "constraint",
                "目标",
                "范围",
                "授权",
                "约束",
                "禁止",
            ],
        ),
    ];

    let mut categorized = vec![Vec::<String>::new(); CATEGORIES.len()];
    for message in messages {
        let Some(role) = message["role"].as_str() else {
            continue;
        };
        let Some(content) = message["content"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let lower = content.to_ascii_lowercase();
        let excerpt = format!(
            "[{role}] {}",
            bounded_excerpt(content, FALLBACK_EXCERPT_CHARS)
        );
        let mut matched = false;
        for (index, (_, markers)) in CATEGORIES.iter().enumerate() {
            if categorized[index].len() < FALLBACK_CATEGORY_LIMIT
                && markers.iter().any(|marker| lower.contains(marker))
            {
                categorized[index].push(excerpt.clone());
                matched = true;
            }
        }
        if !matched && categorized[5].len() < FALLBACK_CATEGORY_LIMIT {
            categorized[5].push(excerpt);
        }
    }

    CATEGORIES
        .iter()
        .enumerate()
        .map(|(index, (heading, _))| {
            let body = if categorized[index].is_empty() {
                "无".to_string()
            } else {
                categorized[index]
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!("## {heading}\n{body}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn bounded_excerpt(content: &str, max_chars: usize) -> String {
    let count = content.chars().count();
    if count <= max_chars {
        return content.to_string();
    }
    format!(
        "{}…[原文共 {count} 字符]",
        content.chars().take(max_chars).collect::<String>()
    )
}

fn estimate(value: &Value) -> u64 {
    value
        .to_string()
        .chars()
        .map(|c| if c > '\u{2e80}' { 1_u64 } else { 1 })
        .sum::<u64>()
        / 3
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_recent_messages_and_user_ledger() {
        let messages = (0..30).map(|i| json!({"role":if i%2==0{"user"}else{"assistant"},"content":"长内容".repeat(200)})).collect::<Vec<_>>();
        let input = prepare(&messages, 1000).expect("should compact");
        assert_eq!(input.retained.len(), 12);
        assert!(input.user_ledger.contains("长内容"));
    }

    #[test]
    fn summary_prompt_requires_security_domain_categories() {
        for required in [
            "已确认漏洞及攻击链",
            "已获取凭证/令牌",
            "已验证有效的 payload",
            "已排除路径与失败死路",
            "资产/端点枚举结论",
            "当前计划进度",
            "授权范围",
        ] {
            assert!(
                SUMMARY_SYSTEM_PROMPT.contains(required),
                "missing {required}"
            );
        }
    }

    #[test]
    fn fallback_summary_is_domain_structured_and_keeps_evidence() {
        let summary = fallback_summary(&[
            json!({"role":"assistant","content":"已确认 SQL 注入，Evidence ev-42，payload UNION SELECT 1"}),
            json!({"role":"tool","content":"timeout: sqlmap failed"}),
            json!({"role":"user","content":"目标 https://shop.test，授权范围仅该主机"}),
        ]);
        assert!(summary.contains("## 已确认漏洞及攻击链"));
        assert!(summary.contains("Evidence ev-42"));
        assert!(summary.contains("UNION SELECT 1"));
        assert!(summary.contains("timeout: sqlmap failed"));
        assert!(summary.contains("https://shop.test"));
        assert!(summary.contains("## 当前计划进度\n无"));
    }
}
