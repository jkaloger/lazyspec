use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde::Serialize;

use crate::engine::config::Config;
use crate::engine::document::split_frontmatter;
use crate::engine::refs::REF_PATTERN;
use crate::engine::store::{extract_id_from_name, Store};
use crate::engine::template::next_number;

#[derive(Debug, Serialize)]
struct FixOutput {
    field_fixes: Vec<FieldFixResult>,
    conflict_fixes: Vec<ConflictFixResult>,
}

#[derive(Debug, Serialize)]
struct FieldFixResult {
    path: String,
    fields_added: Vec<String>,
    written: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ReferenceUpdate {
    pub file: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Serialize)]
struct ConflictFixResult {
    old_path: String,
    new_path: String,
    old_id: String,
    new_id: String,
    references_updated: Vec<ReferenceUpdate>,
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
    let output = collect_all(root, store, config, paths, dry_run);
    let has_fixes = !output.field_fixes.iter().all(|r| r.fields_added.is_empty())
        || !output.conflict_fixes.is_empty();

    if json {
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        println!("{}", json_str);
    } else {
        let human = format_human(&output, dry_run);
        if !human.is_empty() {
            print!("{}", human);
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
    let output = collect_all(root, store, config, paths, dry_run);
    serde_json::to_string_pretty(&output).unwrap()
}

pub fn run_human(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
) -> String {
    let output = collect_all(root, store, config, paths, dry_run);
    format_human(&output, dry_run)
}

fn format_human(output: &FixOutput, dry_run: bool) -> String {
    let mut result = String::new();

    for r in &output.field_fixes {
        if r.fields_added.is_empty() {
            continue;
        }
        let fields = r.fields_added.join(", ");
        if dry_run {
            result.push_str(&format!("Would fix {} (would add: {})\n", r.path, fields));
        } else {
            result.push_str(&format!("Fixed {} (added: {})\n", r.path, fields));
        }
    }

    for c in &output.conflict_fixes {
        if dry_run {
            result.push_str(&format!("Would rename {} -> {}\n", c.old_path, c.new_path));
        } else {
            result.push_str(&format!("Renamed {} -> {}\n", c.old_path, c.new_path));
        }
    }

    result
}

fn collect_all(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
) -> FixOutput {
    let field_fixes = collect_field_fixes(root, store, config, paths, dry_run);
    let conflict_fixes = collect_conflict_fixes(root, store, config, dry_run);
    FixOutput {
        field_fixes,
        conflict_fixes,
    }
}

fn collect_field_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
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
        .filter_map(|p| fix_file(root, config, p, dry_run).ok())
        .collect()
}

fn fix_file(
    root: &Path,
    config: &Config,
    path: &str,
    dry_run: bool,
) -> anyhow::Result<FieldFixResult> {
    let full_path = root.join(path);
    let content = std::fs::read_to_string(&full_path)?;

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
        std::fs::write(&full_path, output)?;
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

fn collect_conflict_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    dry_run: bool,
) -> Vec<ConflictFixResult> {
    let mut id_groups: HashMap<String, Vec<&crate::engine::document::DocMeta>> = HashMap::new();

    for doc in store.all_docs() {
        if doc.virtual_doc {
            continue;
        }
        let filename = doc.path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        let name = if filename == "index.md" {
            doc.path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str())
                .unwrap_or("")
        } else {
            doc.path.file_stem().and_then(|f| f.to_str()).unwrap_or("")
        };
        let id = extract_id_from_name(name);
        id_groups.entry(id).or_default().push(doc);
    }

    let mut results = Vec::new();

    for (id, mut docs) in id_groups {
        if docs.len() < 2 {
            continue;
        }

        // Sort by date ascending; on tie, use filesystem mtime
        docs.sort_by(|a, b| {
            let date_cmp = a.date.cmp(&b.date);
            if date_cmp != std::cmp::Ordering::Equal {
                return date_cmp;
            }
            let mtime_a = std::fs::metadata(root.join(&a.path))
                .and_then(|m| m.modified())
                .ok();
            let mtime_b = std::fs::metadata(root.join(&b.path))
                .and_then(|m| m.modified())
                .ok();
            mtime_a.cmp(&mtime_b)
        });

        // First doc wins, rest are losers that need renumbering
        for loser in &docs[1..] {
            if let Some(mut fix) = renumber_doc(root, loser, &id, config, dry_run) {
                let refs = cascade_references(root, store, &fix.old_path, &fix.new_path, dry_run);
                fix.references_updated = refs;
                results.push(fix);
            }
        }
    }

    results
}

fn renumber_doc(
    root: &Path,
    doc: &crate::engine::document::DocMeta,
    old_id: &str,
    config: &Config,
    dry_run: bool,
) -> Option<ConflictFixResult> {
    let doc_type_prefix = old_id.split('-').next().unwrap_or("");

    // Find the type dir for this prefix
    let type_def = config.types.iter().find(|t| t.prefix.eq_ignore_ascii_case(doc_type_prefix))?;
    let type_dir = root.join(&type_def.dir);

    let new_num = next_number(&type_dir, &type_def.prefix);
    let new_id = format!("{}-{:03}", type_def.prefix, new_num);

    let filename = doc.path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let is_subfolder = filename == "index.md";

    let old_path_str = doc.path.display().to_string();

    if is_subfolder {
        // Subfolder case: rename parent directory
        let parent_rel = doc.path.parent()?;
        let parent_name = parent_rel.file_name().and_then(|f| f.to_str())?;
        let new_dir_name = parent_name.replacen(old_id, &new_id, 1);
        let new_parent_rel = parent_rel.with_file_name(&new_dir_name);
        let new_path_str = new_parent_rel.join("index.md").display().to_string();

        let old_abs = root.join(parent_rel);
        let new_abs = root.join(&new_parent_rel);

        if !dry_run {
            std::fs::rename(&old_abs, &new_abs).ok()?;
            // Update frontmatter title in the renamed index.md
            update_title_in_file(&new_abs.join("index.md"), old_id, &new_id);
        }

        Some(ConflictFixResult {
            old_path: old_path_str,
            new_path: new_path_str,
            old_id: old_id.to_string(),
            new_id,
            references_updated: vec![],
            written: !dry_run,
        })
    } else {
        // Flat file case: rename the file
        let stem = doc.path.file_stem().and_then(|f| f.to_str())?;
        let new_stem = stem.replacen(old_id, &new_id, 1);
        let new_filename = format!("{}.md", new_stem);
        let new_rel = doc.path.with_file_name(&new_filename);
        let new_path_str = new_rel.display().to_string();

        let old_abs = root.join(&doc.path);
        let new_abs = root.join(&new_rel);

        if !dry_run {
            std::fs::rename(&old_abs, &new_abs).ok()?;
            update_title_in_file(&new_abs, old_id, &new_id);
        }

        Some(ConflictFixResult {
            old_path: old_path_str,
            new_path: new_path_str,
            old_id: old_id.to_string(),
            new_id,
            references_updated: vec![],
            written: !dry_run,
        })
    }
}

fn update_title_in_file(path: &Path, old_id: &str, new_id: &str) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let (yaml_str, body) = match split_frontmatter(&content) {
        Ok((y, b)) => (y, b),
        Err(_) => return,
    };

    if !yaml_str.contains(old_id) {
        return;
    }

    let new_yaml = yaml_str.replace(old_id, new_id);
    let output = format!("---\n{}\n---\n{}", new_yaml, body);
    let _ = std::fs::write(path, output);
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

/// Update all documents that reference `old_path` so they point to `new_path` instead.
/// Handles both `related` frontmatter entries and `@ref` body directives.
pub fn cascade_references(
    root: &Path,
    store: &Store,
    old_path: &str,
    new_path: &str,
    dry_run: bool,
) -> Vec<ReferenceUpdate> {
    let mut updates = Vec::new();

    for doc in store.all_docs() {
        let full_path = root.join(&doc.path);
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (yaml_str, body) = match split_frontmatter(&content) {
            Ok((y, b)) => (y, b),
            Err(_) => continue,
        };

        let mut file_updates: Vec<ReferenceUpdate> = Vec::new();
        let file_str = doc.path.display().to_string();

        // Check related entries
        let mut yaml_value: serde_yaml::Value = match serde_yaml::from_str(&yaml_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let mut frontmatter_changed = false;
        if let Some(related_seq) = yaml_value
            .get_mut("related")
            .and_then(|v| v.as_sequence_mut())
        {
            for entry in related_seq.iter_mut() {
                if let Some(mapping) = entry.as_mapping_mut() {
                    for (_key, val) in mapping.iter_mut() {
                        if let Some(s) = val.as_str() {
                            if s.contains(old_path) {
                                let new_val = s.replace(old_path, new_path);
                                file_updates.push(ReferenceUpdate {
                                    file: file_str.clone(),
                                    field: "related".to_string(),
                                    old_value: s.to_string(),
                                    new_value: new_val.clone(),
                                });
                                *val = serde_yaml::Value::String(new_val);
                                frontmatter_changed = true;
                            }
                        }
                    }
                }
            }
        }

        // Check body @ref directives
        let ref_re = Regex::new(REF_PATTERN).unwrap();
        let mut new_body = body.clone();
        let mut body_changed = false;

        for cap in ref_re.captures_iter(&body) {
            let full_match = cap.get(0).unwrap();
            let match_str = full_match.as_str();
            if match_str.contains(old_path) {
                let replaced = match_str.replace(old_path, new_path);
                file_updates.push(ReferenceUpdate {
                    file: file_str.clone(),
                    field: "body".to_string(),
                    old_value: match_str.to_string(),
                    new_value: replaced.clone(),
                });
                new_body = new_body.replace(match_str, &replaced);
                body_changed = true;
            }
        }

        if file_updates.is_empty() {
            continue;
        }

        if !dry_run && (frontmatter_changed || body_changed) {
            let final_body = if body_changed { &new_body } else { &body };
            let new_yaml = match serde_yaml::to_string(&yaml_value) {
                Ok(y) => y,
                Err(_) => continue,
            };
            let output = format!("---\n{}---\n{}", new_yaml, final_body);
            let _ = std::fs::write(&full_path, output);
        }

        updates.extend(file_updates);
    }

    updates
}
