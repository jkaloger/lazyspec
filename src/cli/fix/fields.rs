use std::path::Path;

use crate::engine::config::Config;
use crate::engine::document::split_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use super::FieldFixResult;

const REQUIRED_FIELDS: &[&str] = &["title", "type", "status", "author", "date", "tags"];
const PRIORITY_DEFAULT: &str = "should";

pub(super) fn collect_field_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<FieldFixResult> {
    let file_paths: Vec<String> = if paths.is_empty() {
        store
            .parse_errors()
            .iter()
            .map(|pe| pe.path.display().to_string())
            .collect()
    } else {
        paths.to_vec()
    };

    file_paths
        .iter()
        .filter_map(|p| fix_file(root, config, p, dry_run, fs).ok())
        .collect()
}

fn fix_file(
    root: &Path,
    config: &Config,
    path: &str,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> anyhow::Result<FieldFixResult> {
    let full_path = root.join(path);
    let content = fs.read_to_string(&full_path)?;

    let (mut yaml_text, body) = match split_frontmatter(&content) {
        Ok((y, b)) => (y, b),
        Err(_) => (String::new(), format!("\n{}", content)),
    };

    let mut fields_added = Vec::new();
    for &field in REQUIRED_FIELDS {
        if has_field(&yaml_text, field) {
            continue;
        }
        yaml_text = insert_yaml_field(&yaml_text, field, &default_yaml(field, path, config));
        fields_added.push(field.to_string());
    }

    let written = if !dry_run && !fields_added.is_empty() {
        if !yaml_text.ends_with('\n') {
            yaml_text.push('\n');
        }
        let output = format!("---\n{yaml_text}---{body}");
        fs.write(&full_path, &output)?;
        true
    } else {
        false
    };

    Ok(FieldFixResult {
        path: path.to_string(),
        fields_added,
        written,
    })
}

fn has_field(yaml_text: &str, key: &str) -> bool {
    let prefix = format!("{}:", key);
    yaml_text
        .lines()
        .any(|l| l.trim_start().starts_with(&prefix))
}

fn default_yaml(field: &str, path: &str, config: &Config) -> String {
    match field {
        "title" => yaml_double_quote(&title_from_filename(path)),
        "type" => type_from_path(path, config),
        "status" => "draft".to_string(),
        "author" => yaml_double_quote(&git_author()),
        "date" => chrono::Utc::now().format("%Y-%m-%d").to_string(),
        "tags" => "[]".to_string(),
        _ => "null".to_string(),
    }
}

fn yaml_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn title_from_filename(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");

    let stripped = strip_type_prefix_numeric(stem);
    let words: Vec<&str> = stripped.split('-').collect();
    if words.is_empty() {
        return "untitled".to_string();
    }

    let mut result = String::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            result.push(' ');
        }
        if i == 0 {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_uppercase().next().unwrap_or(first));
                result.extend(chars);
            }
        } else {
            result.push_str(word);
        }
    }
    result
}

fn strip_type_prefix_numeric(stem: &str) -> &str {
    let bytes = stem.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    if i == 0 || i >= len || bytes[i] != b'-' {
        return stem;
    }
    i += 1;

    let digit_start = i;
    while i < len && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digit_start || i >= len || bytes[i] != b'-' {
        return stem;
    }
    i += 1;

    &stem[i..]
}

fn type_from_path(path: &str, config: &Config) -> String {
    let path_obj = Path::new(path);
    if let Some(parent) = path_obj.parent() {
        let parent_str = parent.to_string_lossy();
        for td in &config.documents.types {
            if parent_str == td.dir || parent_str.ends_with(&td.dir) {
                return td.name.clone();
            }
        }
    }
    "rfc".to_string()
}

pub(super) fn insert_yaml_field(yaml_text: &str, key: &str, value_yaml: &str) -> String {
    debug_assert!(!value_yaml.contains('\n'), "value_yaml must be single-line");

    let key_prefix = format!("{}:", key);
    for line in yaml_text.lines() {
        if line.trim_start().starts_with(&key_prefix) {
            return yaml_text.to_string();
        }
    }

    if yaml_text.is_empty() {
        return format!("{}: {}\n", key, value_yaml);
    }

    let mut out = String::with_capacity(yaml_text.len() + key.len() + value_yaml.len() + 4);
    out.push_str(yaml_text);
    if !yaml_text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(key);
    out.push_str(": ");
    out.push_str(value_yaml);
    out.push('\n');
    out
}

fn git_author() -> String {
    std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn collect_priority_fills(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<FieldFixResult> {
    let path_filter: Option<std::collections::HashSet<&str>> = if paths.is_empty() {
        None
    } else {
        Some(paths.iter().map(|s| s.as_str()).collect())
    };

    let mut results = Vec::new();
    for doc in store.all_docs() {
        if doc.validate_ignore {
            continue;
        }
        if doc.priority.is_some() {
            continue;
        }
        let Some(td) = config.type_by_name(doc.doc_type.as_str()) else {
            continue;
        };
        if !td.resolved_requires_priority() {
            continue;
        }

        let path_str = doc.path.to_string_lossy().to_string();
        if let Some(ref filter) = path_filter {
            if !filter.contains(path_str.as_str()) {
                continue;
            }
        }

        if let Ok(result) = fix_priority_file(root, &path_str, dry_run, fs) {
            results.push(result);
        }
    }
    results
}

fn fix_priority_file(
    root: &Path,
    path: &str,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> anyhow::Result<FieldFixResult> {
    let full_path = root.join(path);
    let content = fs.read_to_string(&full_path)?;

    let (mut yaml_text, body) = split_frontmatter(&content)?;

    let mut fields_added = Vec::new();
    if !has_field(&yaml_text, "priority") {
        yaml_text = insert_yaml_field(&yaml_text, "priority", PRIORITY_DEFAULT);
        fields_added.push("priority".to_string());
    }

    let written = if !dry_run && !fields_added.is_empty() {
        if !yaml_text.ends_with('\n') {
            yaml_text.push('\n');
        }
        let output = format!("---\n{yaml_text}---{body}");
        fs.write(&full_path, &output)?;
        true
    } else {
        false
    };

    Ok(FieldFixResult {
        path: path.to_string(),
        fields_added,
        written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use crate::engine::fs::RealFileSystem;
    use std::fs;
    use tempfile::TempDir;

    fn write_doc(root: &Path, rel: &str, content: &str) {
        let full = root.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }

    fn story_no_priority() -> &'static str {
        concat!(
            "---\n",
            "title: \"Login flow\"\n",
            "type: story\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "---\n",
            "Story body content here.\n",
        )
    }

    fn story_with_priority() -> &'static str {
        concat!(
            "---\n",
            "title: \"Login flow\"\n",
            "type: story\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "priority: must\n",
            "---\n",
            "Story body content here.\n",
        )
    }

    fn rfc_no_priority() -> &'static str {
        concat!(
            "---\n",
            "title: \"Some RFC\"\n",
            "type: rfc\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "---\n",
            "RFC body.\n",
        )
    }

    #[test]
    fn ac1_story_without_priority_gets_should_inserted() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/stories/STORY-001-login.md";
        write_doc(root, rel, story_no_priority());

        let config = Config::default();
        let store = Store::load_with_fs(root, &config, &RealFileSystem, None).unwrap();

        let results =
            collect_priority_fills(root, &store, &config, &[], false, &RealFileSystem);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].fields_added, vec!["priority".to_string()]);
        assert!(results[0].written);

        let new_content = fs::read_to_string(root.join(rel)).unwrap();
        let (yaml_str, body) = split_frontmatter(&new_content).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(
            parsed.get("priority").and_then(|v| v.as_str()),
            Some("should")
        );
        assert_eq!(body.trim(), "Story body content here.");
    }

    #[test]
    fn ac2_story_with_existing_priority_is_noop() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/stories/STORY-001-login.md";
        write_doc(root, rel, story_with_priority());
        let original = fs::read_to_string(root.join(rel)).unwrap();

        let config = Config::default();
        let store = Store::load_with_fs(root, &config, &RealFileSystem, None).unwrap();

        let results =
            collect_priority_fills(root, &store, &config, &[], false, &RealFileSystem);

        // Either the doc is filtered (priority Some) so no result is produced,
        // or a result is produced with empty fields_added. Both are acceptable
        // no-ops; either way file content must be unchanged.
        for r in &results {
            assert!(r.fields_added.is_empty());
            assert!(!r.written);
        }
        let after = fs::read_to_string(root.join(rel)).unwrap();
        assert_eq!(original, after);
    }

    #[test]
    fn insert_yaml_field_empty_input_returns_key_value_with_newline() {
        let out = insert_yaml_field("", "priority", "should");
        assert_eq!(out, "priority: should\n");
    }

    #[test]
    fn insert_yaml_field_preserves_trailing_newline() {
        let input = "title: foo\ntype: story\n";
        let out = insert_yaml_field(input, "tags", "[]");
        assert_eq!(out, "title: foo\ntype: story\ntags: []\n");
    }

    #[test]
    fn insert_yaml_field_adds_newline_when_input_lacks_one() {
        let input = "title: foo\ntype: story";
        let out = insert_yaml_field(input, "status", "draft");
        assert_eq!(out, "title: foo\ntype: story\nstatus: draft\n");
    }

    #[test]
    fn insert_yaml_field_skips_when_key_already_present() {
        let input = "title: foo\ntags: [a, b]\n";
        let out = insert_yaml_field(input, "tags", "[]");
        assert_eq!(out, input);
    }

    #[test]
    fn insert_yaml_field_does_not_match_key_prefix_only() {
        let input = "tags_extra: foo\n";
        let out = insert_yaml_field(input, "tags", "[]");
        assert_eq!(out, "tags_extra: foo\ntags: []\n");
    }

    fn run_fix_file(root: &Path, rel: &str) -> FieldFixResult {
        let config = Config::default();
        fix_file(root, &config, rel, false, &RealFileSystem).unwrap()
    }

    #[test]
    fn fix_file_ac1_appends_missing_tags_without_blank_line() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/rfcs/RFC-001-thing.md";
        let original = concat!(
            "---\n",
            "title: \"Some RFC\"\n",
            "type: rfc\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "---\n",
            "Line one of body.\n",
            "\n",
            "Line three after blank.\n",
        );
        write_doc(root, rel, original);

        let result = run_fix_file(root, rel);
        assert!(result.fields_added.contains(&"tags".to_string()));
        assert!(result.written);

        let after = fs::read_to_string(root.join(rel)).unwrap();
        let expected = concat!(
            "---\n",
            "title: \"Some RFC\"\n",
            "type: rfc\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "---\n",
            "Line one of body.\n",
            "\n",
            "Line three after blank.\n",
        );
        assert_eq!(after, expected);
    }

    #[test]
    fn fix_file_ac3_complete_doc_is_byte_identical() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/rfcs/RFC-001-thing.md";
        write_doc(root, rel, rfc_no_priority());
        let before = fs::read_to_string(root.join(rel)).unwrap();

        let result = run_fix_file(root, rel);
        assert!(result.fields_added.is_empty());
        assert!(!result.written);

        let after = fs::read_to_string(root.join(rel)).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn fix_file_ac4_preserves_double_quoted_title() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/rfcs/RFC-001-thing.md";
        let original = concat!(
            "---\n",
            "title: \"Quoted Title\"\n",
            "type: rfc\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "---\n",
            "Body.\n",
        );
        write_doc(root, rel, original);

        run_fix_file(root, rel);

        let after = fs::read_to_string(root.join(rel)).unwrap();
        assert!(after.contains("title: \"Quoted Title\"\n"));
        assert!(after.contains("tags: []\n"));
    }

    #[test]
    fn fix_file_ac5_preserves_single_quoted_author() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/rfcs/RFC-001-thing.md";
        let original = concat!(
            "---\n",
            "title: \"Some RFC\"\n",
            "type: rfc\n",
            "status: draft\n",
            "author: 'jkaloger'\n",
            "date: 2026-01-01\n",
            "---\n",
            "Body.\n",
        );
        write_doc(root, rel, original);

        run_fix_file(root, rel);

        let after = fs::read_to_string(root.join(rel)).unwrap();
        assert!(after.contains("author: 'jkaloger'\n"));
    }

    #[test]
    fn fix_file_ac6_preserves_existing_empty_tags_when_inserting_author() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/rfcs/RFC-001-thing.md";
        let original = concat!(
            "---\n",
            "title: \"Some RFC\"\n",
            "type: rfc\n",
            "status: draft\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "---\n",
            "Body.\n",
        );
        write_doc(root, rel, original);

        let result = run_fix_file(root, rel);
        assert!(result.fields_added.contains(&"author".to_string()));

        let after = fs::read_to_string(root.join(rel)).unwrap();
        assert!(after.contains("tags: []\n"));
        assert!(!after.contains("\n\n---\n"), "no extra blank line before closing fence");
    }

    #[test]
    fn fix_priority_ac2_byte_level_diff() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/stories/STORY-001-login.md";
        let original = concat!(
            "---\n",
            "title: \"Login flow\"\n",
            "type: story\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "---\n",
            "Story body line one.\n",
            "\n",
            "Story body line three.\n",
        );
        write_doc(root, rel, original);
        let before = fs::read_to_string(root.join(rel)).unwrap();

        let config = Config::default();
        let store = Store::load_with_fs(root, &config, &RealFileSystem, None).unwrap();

        let results =
            collect_priority_fills(root, &store, &config, &[], false, &RealFileSystem);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].fields_added, vec!["priority".to_string()]);
        assert!(results[0].written);

        let after = fs::read_to_string(root.join(rel)).unwrap();
        let expected = concat!(
            "---\n",
            "title: \"Login flow\"\n",
            "type: story\n",
            "status: draft\n",
            "author: \"alice\"\n",
            "date: 2026-01-01\n",
            "tags: []\n",
            "priority: should\n",
            "---\n",
            "Story body line one.\n",
            "\n",
            "Story body line three.\n",
        );
        assert_eq!(after, expected);
        assert_eq!(after.len(), before.len() + "priority: should\n".len());
    }

    #[test]
    fn ac3_rfc_without_priority_is_noop() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let rel = "docs/rfcs/RFC-001-thing.md";
        write_doc(root, rel, rfc_no_priority());
        let original = fs::read_to_string(root.join(rel)).unwrap();

        let config = Config::default();
        let store = Store::load_with_fs(root, &config, &RealFileSystem, None).unwrap();

        let results =
            collect_priority_fills(root, &store, &config, &[], false, &RealFileSystem);

        assert!(
            results.is_empty(),
            "rfc should not appear in priority fill results"
        );
        let after = fs::read_to_string(root.join(rel)).unwrap();
        assert_eq!(original, after);
    }
}
