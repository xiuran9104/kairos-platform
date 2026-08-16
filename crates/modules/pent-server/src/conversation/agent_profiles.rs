//! Markdown-defined specialist Agent profiles, compatible with DesRed's agents directory idea.

use std::path::{Path, PathBuf};

pub use kairos_platform_pent_runtime::ports::RuntimeAgentProfile as AgentProfile;
use kairos_platform_pent_runtime::ports::RoleResolver;

pub struct MarkdownRoleResolver;

impl RoleResolver for MarkdownRoleResolver {
    fn resolve(&self, role: &str) -> anyhow::Result<Option<AgentProfile>> {
        load(role)
    }
}

/// Root directory holding the markdown agent profiles.
pub(crate) fn agents_dir() -> PathBuf {
    if let Ok(configured) = std::env::var("PENTA_AGENTS_DIR") {
        return PathBuf::from(configured);
    }
    let working_directory = PathBuf::from("agents");
    if working_directory.is_dir() {
        working_directory
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../agents")
    }
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

/// List every valid Markdown-backed specialist profile available to the
/// runtime. This is also used to build the model's `agent_spawn.role` schema,
/// keeping tool disclosure in sync with the profiles on disk.
pub fn list() -> anyhow::Result<Vec<AgentProfile>> {
    let root = agents_dir();
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut profiles = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if is_valid_id(id) {
            profiles.push(parse(&path, id)?);
        }
    }
    profiles.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(profiles)
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
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_role() {
        assert!(load("../secret").unwrap().is_none());
    }

    #[test]
    fn all_shipped_profiles_parse() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../agents");
        assert!(dir.is_dir(), "agents dir missing: {}", dir.display());
        let mut count = 0usize;
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let profile = parse(&path, id).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(!profile.id.is_empty(), "{}: missing id", path.display());
            assert!(!profile.name.is_empty(), "{}: missing name", path.display());
            assert!(!profile.instruction.is_empty(), "{}: empty body", path.display());
            count += 1;
        }
        assert!(count >= 12, "expected at least 12 shipped profiles, got {count}");
    }
}
