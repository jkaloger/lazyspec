use std::collections::{HashMap, HashSet};
use std::path::Path;

use regex::Regex;

use crate::cli::RenumberFormat;
use crate::engine::config::{Config, SqidsConfig};
use crate::engine::document::split_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::refs::REF_PATTERN;
use crate::engine::store::Store;
use crate::engine::template::shuffle_alphabet;

use super::{ExternalReference, ReferenceUpdate, RenumberFixResult, RenumberOutput};

pub(super) fn collect_renumber_output(
    root: &Path,
    store: &Store,
    config: &Config,
    format: &RenumberFormat,
    doc_type: Option<&str>,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> RenumberOutput {
    let format_str = match format {
        RenumberFormat::Sqids => "sqids",
        RenumberFormat::Incremental => "incremental",
    };

    let changes = plan_renumbering(root, store, config, format, doc_type, dry_run, fs);
    let external_references = scan_external_references(root, store, config, &changes, fs);

    RenumberOutput {
        format: format_str.to_string(),
        doc_type: doc_type.map(|s| s.to_string()),
        dry_run,
        changes,
        external_references,
    }
}

fn is_incremental_id(id_segment: &str) -> bool {
    !id_segment.is_empty() && id_segment.chars().all(|c| c.is_ascii_digit())
}

fn build_sqids_encoder(sqids_config: &SqidsConfig) -> Result<sqids::Sqids, sqids::Error> {
    let alphabet = shuffle_alphabet(&sqids_config.salt);
    sqids::Sqids::builder()
        .alphabet(alphabet)
        .min_length(sqids_config.min_length)
        .blocklist(HashSet::new())
        .build()
}

fn plan_renumbering(
    root: &Path,
    store: &Store,
    config: &Config,
    format: &RenumberFormat,
    doc_type_filter: Option<&str>,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<RenumberFixResult> {
    let target_types: Vec<&crate::engine::config::TypeDef> = config
        .documents
        .types
        .iter()
        .filter(|t| {
            if let Some(filter) = doc_type_filter {
                t.name.eq_ignore_ascii_case(filter) || t.prefix.eq_ignore_ascii_case(filter)
            } else {
                true
            }
        })
        .collect();

    let mut all_renames: Vec<RenumberFixResult> = Vec::new();

    for type_def in &target_types {
        let prefix = &type_def.prefix;

        let mut type_docs: Vec<&crate::engine::document::DocMeta> = store
            .all_docs()
            .into_iter()
            .filter(|d| {
                if d.virtual_doc {
                    return false;
                }
                d.display_name().starts_with(&format!("{}-", prefix))
            })
            .collect();

        type_docs.sort_by(|a, b| a.path.cmp(&b.path));

        match format {
            RenumberFormat::Sqids => {
                let sqids_config = match config.documents.sqids.as_ref() {
                    Some(c) => c,
                    None => continue,
                };
                let encoder = match build_sqids_encoder(sqids_config) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                for doc in &type_docs {
                    let id = doc.display_name();
                    let id_segment = id.strip_prefix(&format!("{}-", prefix)).unwrap_or("");

                    if !is_incremental_id(id_segment) {
                        continue;
                    }

                    let numeric: u64 = id_segment.parse().unwrap_or(0);
                    let sqid = match encoder.encode(&[numeric]) {
                        Ok(s) => s.to_lowercase(),
                        Err(_) => continue,
                    };
                    let new_id = format!("{}-{}", prefix, sqid);

                    if let Some(rename) = build_rename(root, doc, id, &new_id, dry_run, fs) {
                        all_renames.push(rename);
                    }
                }
            }
            RenumberFormat::Incremental => {
                let max_existing: u32 = type_docs
                    .iter()
                    .filter_map(|d| {
                        let id = d.display_name();
                        let id_segment = id.strip_prefix(&format!("{}-", prefix)).unwrap_or("");
                        if is_incremental_id(id_segment) {
                            id_segment.parse::<u32>().ok()
                        } else {
                            None
                        }
                    })
                    .max()
                    .unwrap_or(0);

                let sqids_docs: Vec<&&crate::engine::document::DocMeta> = type_docs
                    .iter()
                    .filter(|d| {
                        let id = d.display_name();
                        let id_segment = id.strip_prefix(&format!("{}-", prefix)).unwrap_or("");
                        !is_incremental_id(id_segment)
                    })
                    .collect();

                for (i, doc) in sqids_docs.iter().enumerate() {
                    let id = doc.display_name();

                    let new_num = max_existing + (i as u32) + 1;
                    let new_id = format!("{}-{:03}", prefix, new_num);

                    if id == new_id {
                        continue;
                    }

                    if let Some(rename) = build_rename(root, doc, id, &new_id, dry_run, fs) {
                        all_renames.push(rename);
                    }
                }
            }
        }
    }

    if !all_renames.is_empty() {
        let path_map: HashMap<String, String> = all_renames
            .iter()
            .map(|r| (r.old_path.clone(), r.new_path.clone()))
            .collect();

        for rename in &mut all_renames {
            let refs = cascade_references(root, store, &rename.old_path, &rename.new_path, dry_run, fs);
            rename.references_updated = refs;
        }

        if !dry_run {
            for rename in &all_renames {
                let new_abs = root.join(&rename.new_path);
                let content = match fs.read_to_string(&new_abs) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let mut updated = content.clone();
                for (old_p, new_p) in &path_map {
                    if old_p == &rename.old_path {
                        continue;
                    }
                    updated = updated.replace(old_p.as_str(), new_p.as_str());
                }
                if updated != content {
                    let _ = fs.write(&new_abs, &updated);
                }
            }
        }
    }

    all_renames
}

pub fn scan_external_references(
    root: &Path,
    store: &Store,
    config: &Config,
    changes: &[RenumberFixResult],
    fs: &dyn FileSystem,
) -> Vec<ExternalReference> {
    if changes.is_empty() {
        return vec![];
    }

    let old_names: Vec<String> = changes
        .iter()
        .map(|c| {
            Path::new(&c.old_path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&c.old_path)
                .to_string()
        })
        .collect();

    let managed_dirs: HashSet<String> = config.documents.types.iter().map(|t| t.dir.clone()).collect();

    let managed_paths: HashSet<String> = store
        .all_docs()
        .iter()
        .map(|d| d.path.display().to_string())
        .collect();

    let mut refs = Vec::new();
    scan_dir_for_references(root, root, &managed_dirs, &managed_paths, &old_names, &mut refs, fs);
    refs
}

fn scan_dir_for_references(
    root: &Path,
    dir: &Path,
    managed_dirs: &HashSet<String>,
    managed_paths: &HashSet<String>,
    old_names: &[String],
    refs: &mut Vec<ExternalReference>,
    fs: &dyn FileSystem,
) {
    let entries = match fs.read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    const NOISE_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "dist", "build", ".hg"];

    for path in entries {
        if fs.is_dir(&path) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if NOISE_DIRS.contains(&name) {
                    continue;
                }
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_str = rel.display().to_string();
            if managed_dirs.contains(&rel_str) {
                continue;
            }
            scan_dir_for_references(root, &path, managed_dirs, managed_paths, old_names, refs, fs);
            continue;
        }

        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        let is_scannable = filename.ends_with(".md")
            || filename.ends_with(".wiki")
            || filename.starts_with("README");

        if !is_scannable {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = rel.display().to_string();
        if managed_paths.contains(&rel_str) {
            continue;
        }

        let content = match fs.read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            for old_name in old_names {
                if line.contains(old_name.as_str()) {
                    refs.push(ExternalReference {
                        file: rel_str.clone(),
                        old_name: old_name.clone(),
                        line: line_num + 1,
                    });
                }
            }
        }
    }
}

fn build_rename(
    root: &Path,
    doc: &crate::engine::document::DocMeta,
    old_id: &str,
    new_id: &str,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Option<RenumberFixResult> {
    let filename = doc.path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let is_subfolder = filename == "index.md";
    let old_path_str = doc.path.display().to_string();

    if is_subfolder {
        let parent_rel = doc.path.parent()?;
        let parent_name = parent_rel.file_name().and_then(|f| f.to_str())?;
        let new_dir_name = parent_name.replacen(old_id, new_id, 1);
        let new_parent_rel = parent_rel.with_file_name(&new_dir_name);
        let new_path_str = new_parent_rel.join("index.md").display().to_string();

        let old_abs = root.join(parent_rel);
        let new_abs = root.join(&new_parent_rel);

        if !dry_run {
            fs.rename(&old_abs, &new_abs).ok()?;
            update_title_in_file(&new_abs.join("index.md"), old_id, new_id, fs);
        }

        Some(RenumberFixResult {
            old_path: old_path_str,
            new_path: new_path_str,
            old_id: old_id.to_string(),
            new_id: new_id.to_string(),
            references_updated: vec![],
            written: !dry_run,
        })
    } else {
        let stem = doc.path.file_stem().and_then(|f| f.to_str())?;
        let new_stem = stem.replacen(old_id, new_id, 1);
        let new_filename = format!("{}.md", new_stem);
        let new_rel = doc.path.with_file_name(&new_filename);
        let new_path_str = new_rel.display().to_string();

        let old_abs = root.join(&doc.path);
        let new_abs = root.join(&new_rel);

        if !dry_run {
            fs.rename(&old_abs, &new_abs).ok()?;
            update_title_in_file(&new_abs, old_id, new_id, fs);
        }

        Some(RenumberFixResult {
            old_path: old_path_str,
            new_path: new_path_str,
            old_id: old_id.to_string(),
            new_id: new_id.to_string(),
            references_updated: vec![],
            written: !dry_run,
        })
    }
}

fn update_title_in_file(path: &Path, old_id: &str, new_id: &str, fs: &dyn FileSystem) {
    let content = match fs.read_to_string(path) {
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
    let _ = fs.write(path, &output);
}

pub fn cascade_references(
    root: &Path,
    store: &Store,
    old_path: &str,
    new_path: &str,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<ReferenceUpdate> {
    let mut updates = Vec::new();

    for doc in store.all_docs() {
        let full_path = root.join(&doc.path);
        let content = match fs.read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (yaml_str, body) = match split_frontmatter(&content) {
            Ok((y, b)) => (y, b),
            Err(_) => continue,
        };

        let mut file_updates: Vec<ReferenceUpdate> = Vec::new();
        let file_str = doc.path.display().to_string();

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
            let _ = fs.write(&full_path, &output);
        }

        updates.extend(file_updates);
    }

    updates
}
