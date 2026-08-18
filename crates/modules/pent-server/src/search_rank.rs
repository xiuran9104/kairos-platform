//! Small deterministic lexical ranker shared by tool and Skill discovery.
//! It deliberately handles CJK bigrams because PostgreSQL's `simple` text
//! search treats an unsegmented Chinese sentence as one token.

#[derive(Debug, Clone)]
pub(crate) struct SearchQuery {
    normalized: String,
    terms: Vec<String>,
}

impl SearchQuery {
    pub(crate) fn new(query: &str) -> Self {
        Self {
            normalized: query.trim().to_lowercase(),
            terms: query_terms(query),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub(crate) fn score(&self, fields: &[(&str, f32)]) -> f32 {
        if self.normalized.is_empty() || self.terms.is_empty() {
            return 0.0;
        }
        fields
            .iter()
            .map(|(field, weight)| {
                let field = field.to_lowercase();
                let mut score = 0.0;
                if field == self.normalized {
                    score += weight * 8.0;
                } else if field.contains(&self.normalized) {
                    score += weight * 4.0;
                }
                for term in &self.terms {
                    if contains_term(&field, term) {
                        let specificity = (term.chars().count().min(8) as f32 / 4.0).max(1.0);
                        score += weight * specificity;
                    }
                }
                score
            })
            .sum()
    }
}

fn contains_term(field: &str, term: &str) -> bool {
    if term.chars().any(is_cjk) {
        return field.contains(term);
    }
    field
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .any(|token| token == term || (term.len() >= 3 && token.starts_with(term)))
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut ascii = String::new();
    let mut cjk = Vec::new();

    let flush_ascii = |buffer: &mut String, output: &mut Vec<String>| {
        let term = buffer.trim_matches(['_', '-']);
        if term.chars().count() >= 2 {
            output.push(term.to_string());
        }
        buffer.clear();
    };
    let flush_cjk = |buffer: &mut Vec<char>, output: &mut Vec<String>| {
        if buffer.len() >= 2 {
            if buffer.len() <= 12 {
                output.push(buffer.iter().collect());
            }
            output.extend(
                buffer
                    .windows(2)
                    .map(|window| window.iter().collect::<String>()),
            );
        }
        buffer.clear();
    };

    for character in query.to_lowercase().chars() {
        if is_cjk(character) {
            flush_ascii(&mut ascii, &mut terms);
            cjk.push(character);
        } else if character.is_alphanumeric() || matches!(character, '_' | '-') {
            flush_cjk(&mut cjk, &mut terms);
            ascii.push(character);
        } else {
            flush_ascii(&mut ascii, &mut terms);
            flush_cjk(&mut cjk, &mut terms);
        }
    }
    flush_ascii(&mut ascii, &mut terms);
    flush_cjk(&mut cjk, &mut terms);
    terms.sort();
    terms.dedup();
    terms
}

fn is_cjk(character: char) -> bool {
    matches!(
        character,
        '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{f900}'..='\u{faff}'
            | '\u{3040}'..='\u{30ff}'
            | '\u{ac00}'..='\u{d7af}'
    )
}

#[cfg(test)]
mod tests {
    use super::SearchQuery;

    #[test]
    fn chinese_bigrams_match_reordered_security_terms() {
        let query = SearchQuery::new("扫描靶场漏洞");
        let nuclei = query.score(&[("Nuclei 模板化漏洞扫描器", 4.0)]);
        let ssl = query.score(&[("SSL/TLS 配置扫描器", 4.0)]);
        assert!(nuclei > ssl, "漏洞+扫描应优先于仅命中扫描的工具");
    }

    #[test]
    fn weighted_exact_fields_beat_description_only_hits() {
        let query = SearchQuery::new("XSS validation");
        let exact = query.score(&[("xss-validation", 10.0), ("XSS 验证", 8.0)]);
        let incidental = query.score(&[("general-web-audit", 10.0), ("包含 XSS validation", 2.0)]);
        assert!(exact > incidental);
    }

    #[test]
    fn ascii_terms_do_not_match_inside_unrelated_words() {
        let query = SearchQuery::new("SQL validation");
        let sqli = query.score(&[("sqli-validation", 10.0)]);
        let nosql = query.score(&[("nosql-injection", 10.0)]);
        assert!(sqli > nosql);
    }
}
