use std::path::Path;

use serde::Serialize;

use crate::engine::config::Config;
use crate::engine::document::split_frontmatter;
use crate::engine::store::Store;

#[derive(Debug, Serialize)]
struct FixResult {
    path: String,
    fields_added: Vec<String>,
    written: bool,
}

const REQUIRED_FIELDS: &[&str] = &["title", "type", "status", "author", "date", "tags"];

pub fn run(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    json: bool,
) -> i32 {
    let results = collect_results(root, store, config, paths, dry_run);
    let has_fixes = results.iter().any(|r| !r.fields_added.is_empty());

    if json {
        let output = serde_json::to_string_pretty(&results).unwrap();
        println!("{}", output);
    } else {
        let output = format_human(&results, dry_run);
        if !output.is_empty() {
            print!("{}", output);
        }
    }

    if has_fixes { 0 } else { 1 }
}

pub fn run_json(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
) -> String {
    let results = collect_results(root, store, config, paths, dry_run);
    serde_json::to_string_pretty(&results).unwrap()
}

pub fn run_human(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
) -> String {
    let results = collect_results(root, store, config, paths, dry_run);
    format_human(&results, dry_run)
}

fn format_human(results: &[FixResult], dry_run: bool) -> String {
    let mut output = String::new();

    for r in results {
        if r.fields_added.is_empty() {
            continue;
        }
        let fields = r.fields_added.join(", ");
        if dry_run {
            output.push_str(&format!("Would fix {} (would add: {})\n", r.path, fields));
        } else {
            output.push_str(&format!("Fixed {} (added: {})\n", r.path, fields));
        }
    }

    output
}

fn collect_results(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
) -> Vec<FixResult> {
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
        .filter_map(|p| fix_file(root, config, p, dry_run).ok())
        .collect()
}

fn fix_file(root: &Path, config: &Config, path: &str, dry_run: bool) -> anyhow::Result<FixResult> {
    let full_path = root.join(path);
    let content = std::fs::read_to_string(&full_path)?;

    let (yaml_str, body) = match split_frontmatter(&content) {
        Ok((y, b)) => (y, b),
        Err(_) => {
            // No frontmatter at all: treat entire content as body
            (String::new(), content.clone())
        }
    };

    let mut mapping = if yaml_str.is_empty() {
        serde_yaml::Mapping::new()
    } else {
        let value: serde_yaml::Value = serde_yaml::from_str(&yaml_str)?;
        match value {
            serde_yaml::Value::Mapping(m) => m,
            _ => serde_yaml::Mapping::new(),
        }
    };

    let mut fields_added = Vec::new();

    for &field in REQUIRED_FIELDS {
        let key = serde_yaml::Value::String(field.to_string());
        if mapping.contains_key(&key) {
            continue;
        }

        let value = default_for_field(field, path, config);
        mapping.insert(key, value);
        fields_added.push(field.to_string());
    }

    let written = if !dry_run && !fields_added.is_empty() {
        let new_yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(mapping))?;
        let output = format!("---\n{}---\n{}", new_yaml, body);
        std::fs::write(&full_path, output)?;
        true
    } else {
        false
    };

    Ok(FixResult {
        path: path.to_string(),
        fields_added,
        written,
    })
}

fn default_for_field(field: &str, path: &str, config: &Config) -> serde_yaml::Value {
    match field {
        "title" => serde_yaml::Value::String(title_from_filename(path)),
        "type" => serde_yaml::Value::String(type_from_path(path, config)),
        "status" => serde_yaml::Value::String("draft".to_string()),
        "author" => serde_yaml::Value::String(git_author()),
        "date" => serde_yaml::Value::String(
            chrono::Utc::now().format("%Y-%m-%d").to_string(),
        ),
        "tags" => serde_yaml::Value::Sequence(vec![]),
        _ => serde_yaml::Value::Null,
    }
}

fn title_from_filename(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");

    // Strip type prefix like "RFC-001-" or "STORY-002-"
    let stripped = strip_type_prefix(stem);
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

fn strip_type_prefix(stem: &str) -> &str {
    // Match patterns like "RFC-001-", "STORY-002-", "ADR-003-", "ITERATION-001-"
    let bytes = stem.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Skip uppercase letters
    while i < len && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    if i == 0 || i >= len || bytes[i] != b'-' {
        return stem;
    }
    i += 1; // skip first dash

    // Skip digits
    let digit_start = i;
    while i < len && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digit_start || i >= len || bytes[i] != b'-' {
        return stem;
    }
    i += 1; // skip second dash

    &stem[i..]
}

fn type_from_path(path: &str, config: &Config) -> String {
    let path_obj = Path::new(path);
    if let Some(parent) = path_obj.parent() {
        let parent_str = parent.to_string_lossy();
        for td in &config.types {
            if parent_str == td.dir || parent_str.ends_with(&td.dir) {
                return td.name.clone();
            }
        }
    }
    "rfc".to_string()
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
