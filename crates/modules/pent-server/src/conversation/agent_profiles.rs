//! Markdown-defined specialist Agent profiles, compatible with DesRed's agents directory idea.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AgentProfile { pub id: String, pub name: String, pub description: String, pub instruction: String }

/// Root directory holding the markdown agent profiles.
pub(crate) fn agents_dir() -> PathBuf {
    std::env::var("PENTA_AGENTS_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("agents"))
}

/// Agent ids double as file names; restrict them to path-safe characters.
pub(crate) fn is_valid_id(id: &str) -> bool {
    let id = id.trim();
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

pub fn load(role: &str) -> anyhow::Result<Option<AgentProfile>> {
    let root = agents_dir();
    let safe = role.trim();
    if !is_valid_id(safe) { return Ok(None); }
    let path = root.join(format!("{safe}.md"));
    if !path.exists() { return Ok(None); }
    parse(&path, safe).map(Some)
}

pub(crate) fn parse(path: &Path, fallback_id: &str) -> anyhow::Result<AgentProfile> {
    let content = std::fs::read_to_string(path)?;
    let (header, body) = if let Some(rest) = content.strip_prefix("---\n") {
        rest.split_once("\n---\n").unwrap_or(("", rest))
    } else { ("", content.as_str()) };
    let field = |name: &str| header.lines().find_map(|line| line.split_once(':').filter(|(key, _)| key.trim()==name).map(|(_, value)| value.trim().trim_matches('"').to_string()));
    Ok(AgentProfile {
        id: field("id").unwrap_or_else(|| fallback_id.into()),
        name: field("name").unwrap_or_else(|| fallback_id.into()),
        description: field("description").unwrap_or_default(),
        instruction: body.trim().to_string(),
    })
}

#[cfg(test)]
mod tests { use super::*; #[test] fn rejects_path_traversal_role() { assert!(load("../secret").unwrap().is_none()); } }
