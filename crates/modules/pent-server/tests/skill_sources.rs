use std::{fs, path::Path};

fn collect_skill_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).expect("read skill source directory") {
        let path = entry.expect("read skill source entry").path();
        if path.is_dir() {
            collect_skill_files(&path, files);
        } else if path.file_name().is_some_and(|name| name == "SKILL.md") {
            files.push(path);
        }
    }
}

fn frontmatter(content: &str) -> &str {
    let content = content
        .strip_prefix("---\n")
        .expect("SKILL.md must start with YAML frontmatter");
    let end = content
        .find("\n---\n")
        .expect("SKILL.md frontmatter must have a closing delimiter");
    &content[..end]
}

fn has_non_empty_description(frontmatter: &str) -> bool {
    let mut lines = frontmatter.lines();
    while let Some(line) = lines.next() {
        let Some(value) = line.strip_prefix("description:") else {
            continue;
        };
        let value = value.trim();
        if !matches!(value, "|" | ">" | "|-" | ">-") {
            return !value.trim_matches(['\'', '"']).trim().is_empty();
        }
        return lines
            .take_while(|line| line.starts_with(' ') || line.starts_with('\t'))
            .any(|line| !line.trim().is_empty());
    }
    false
}

#[test]
fn bundled_skill_sources_have_non_empty_descriptions() {
    let skill_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/pentagi-wiki/skills");
    let mut files = Vec::new();
    collect_skill_files(&skill_root, &mut files);
    assert!(!files.is_empty(), "expected bundled SKILL.md sources");

    let mut invalid = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).expect("read SKILL.md");
        if !has_non_empty_description(frontmatter(&content)) {
            invalid.push(path);
        }
    }

    assert!(
        invalid.is_empty(),
        "SKILL.md files with missing descriptions:\n{}",
        invalid
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
