//! Structured claim gate — verifies that concrete assertions in a candidate
//! FINAL are verbatim-projectable into the run's cited Evidence texts.
//!
//! This is the deterministic anti-hallucination gate: when the model claims a
//! verified finding and quotes concrete tokens (payloads, markers, flags in
//! backticks or flag-like strings), each token must appear character-for-
//! character in at least one cited Evidence source. Claims without quotable
//! tokens pass through unchanged (nothing to project).

/// Result of assessing a candidate FINAL against cited evidence texts.
#[derive(Debug, Default)]
pub struct ClaimAssessment {
    /// Assertions not found verbatim in any cited evidence text.
    pub ungrounded: Vec<String>,
}

impl ClaimAssessment {
    /// Model-facing rejection reason when ungrounded assertions exist.
    pub fn rejection(&self) -> Option<String> {
        if self.ungrounded.is_empty() {
            return None;
        }
        let shown: Vec<String> = self
            .ungrounded
            .iter()
            .take(3)
            .map(|token| format!("`{token}`"))
            .collect();
        Some(format!(
            "以下关键断言未在任何被引用的 Evidence 正文中逐字出现：{}。\
             事实性结论的关键字符串（payload、特征标记、flag 等）必须能从所引用证据中逐字投影；\
             请改用真实出现过的字符串、引用正确的 Evidence ID，或将该结论降级为未验证线索。",
            shown.join("、")
        ))
    }
}

const MIN_ASSERTION_CHARS: usize = 6;
const MAX_ASSERTION_CHARS: usize = 200;
const MAX_ASSERTIONS: usize = 20;

/// Assess a candidate FINAL against `(evidence_id, evidence_text)` pairs.
/// `cited_ids` limits matching to evidence the candidate actually references.
pub fn assess_claims(candidate: &str, cited_evidence: &[(String, String)]) -> ClaimAssessment {
    let assertions = extract_assertions(candidate);
    let ungrounded = assertions
        .iter()
        .filter(|token| {
            !cited_evidence
                .iter()
                .any(|(_, text)| text.contains(token.as_str()))
        })
        .cloned()
        .collect();
    ClaimAssessment { ungrounded }
}

/// Extract quotable assertion tokens: backtick code spans and flag-like
/// strings (e.g. `flag{...}`, `SSTI{...}`). Markdown formatting characters
/// and prose are ignored.
pub fn extract_assertions(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else {
            break;
        };
        let raw_span = &after[..end];
        rest = &after[end + 1..];
        if raw_span.contains('\n') {
            continue;
        }
        push_assertion(&mut tokens, raw_span.trim());
    }
    for token in flag_like_tokens(content) {
        push_assertion(&mut tokens, &token);
    }
    tokens
}

fn push_assertion(tokens: &mut Vec<String>, token: &str) {
    let char_count = token.chars().count();
    if !(MIN_ASSERTION_CHARS..=MAX_ASSERTION_CHARS).contains(&char_count) {
        return;
    }
    if tokens.len() >= MAX_ASSERTIONS || tokens.iter().any(|existing| existing == token) {
        return;
    }
    tokens.push(token.to_string());
}

/// Flag-style tokens such as `flag{...}` / `FLAG{...}` / `ctf{...}`.
fn flag_like_tokens(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in content.split(|c: char| c.is_whitespace() || c == '`' || c == '"' || c == '\'') {
        let word = word.trim_matches(|c: char| {
            matches!(
                c,
                '(' | ')'
                    | '['
                    | ']'
                    | '<'
                    | '>'
                    | ','
                    | '.'
                    | ';'
                    | ':'
                    | '。'
                    | '，'
                    | '；'
                    | '：'
                    | '！'
                    | '？'
            )
        });
        let lower = word.to_ascii_lowercase();
        let looks_flag = (lower.starts_with("flag{")
            || lower.starts_with("ctf{")
            || lower.contains("{") && lower.ends_with("}"))
            && word.ends_with('}')
            && word.chars().count() >= MIN_ASSERTION_CHARS;
        if looks_flag {
            tokens.push(word.to_string());
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cited(texts: &[&str]) -> Vec<(String, String)> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| (format!("ev-{index}"), text.to_string()))
            .collect()
    }

    #[test]
    fn extracts_backtick_spans_and_flags() {
        let tokens = extract_assertions(
            "注入 payload `' OR 1=1--` 得到 `sql syntax error`，flag 为 flag{abc123}。",
        );
        assert!(tokens.contains(&"' OR 1=1--".to_string()));
        assert!(tokens.contains(&"sql syntax error".to_string()));
        assert!(tokens.contains(&"flag{abc123}".to_string()));
    }

    #[test]
    fn skips_short_and_multiline_spans() {
        let tokens = extract_assertions("`200` `a` ```\nmultiline\n``` 普通文本");
        assert!(tokens.is_empty());
    }

    #[test]
    fn grounded_claims_pass() {
        let evidence = cited(&["HTTP/1.1 200 OK\nsql syntax error near '' OR 1=1--"]);
        let assessment = assess_claims(
            "结论：存在注入，payload `' OR 1=1--`，特征 `sql syntax error`",
            &evidence,
        );
        assert!(assessment.ungrounded.is_empty());
        assert!(assessment.rejection().is_none());
    }

    #[test]
    fn fabricated_token_is_rejected_with_details() {
        let evidence = cited(&["HTTP/1.1 200 OK\nplain page"]);
        let assessment = assess_claims("漏洞已验证，marker `pwned-by-agent-777`", &evidence);
        assert_eq!(
            assessment.ungrounded,
            vec!["pwned-by-agent-777".to_string()]
        );
        let rejection = assessment.rejection().unwrap();
        assert!(rejection.contains("pwned-by-agent-777"));
        assert!(rejection.contains("逐字投影"));
    }

    #[test]
    fn no_quotable_tokens_passes() {
        let evidence = cited(&["anything"]);
        let assessment = assess_claims("目标存在注入风险，建议进一步验证。", &evidence);
        assert!(extract_assertions("目标存在注入风险，建议进一步验证。").is_empty());
        assert!(assessment.rejection().is_none());
    }

    #[test]
    fn unmatched_citation_cannot_ground() {
        let evidence = cited(&["evidence A contains `token-a`"]);
        let assessment = assess_claims("引用 `token-b`", &evidence);
        assert_eq!(assessment.ungrounded.len(), 1);
    }
}
