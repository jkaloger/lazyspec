use std::path::Path;

use crate::engine::config::Config;
use crate::engine::document::split_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use super::FieldFixResult;

const REQUIRED_FIELDS: &[&str] = &["title", "type", "status", "author", "date", "tags"];

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

    let (yaml_str, body) = match split_frontmatter(&content) {
        Ok((y, b)) => (y, b),
        Err(_) => (String::new(), content.clone()),
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

fn default_for_field(field: &str, path: &str, config: &Config) -> serde_yaml::Value {
    match field {
        "title" => serde_yaml::Value::String(title_from_filename(path)),
        "type" => serde_yaml::Value::String(type_from_path(path, config)),
        "status" => serde_yaml::Value::String("draft".to_string()),
        "author" => serde_yaml::Value::String(git_author()),
        "date" => serde_yaml::Value::String(chrono::Utc::now().format("%Y-%m-%d").to_string()),
        "tags" => serde_yaml::Value::Sequence(vec![]),
        _ => serde_yaml::Value::Null,
    }
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
