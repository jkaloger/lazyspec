use std::collections::HashMap;
use std::path::Path;

use crate::engine::config::{Config, NumberingStrategy};
use crate::engine::document::split_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::{extract_id_from_name, Store};
use crate::engine::template::{next_number, next_sqids_id};

use super::renumber::cascade_references;
use super::ConflictFixResult;

pub(super) fn collect_conflict_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    dry_run: bool,
    fs: &dyn FileSystem,
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

        for loser in &docs[1..] {
            if let Some(mut fix) = renumber_doc(root, loser, &id, config, dry_run, fs) {
                let refs = cascade_references(root, store, &fix.old_id, &fix.new_id, dry_run, fs);
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
    fs: &dyn FileSystem,
) -> Option<ConflictFixResult> {
    let doc_type_prefix = old_id.split('-').next().unwrap_or("");

    let type_def = config.documents.types.iter().find(|t| t.prefix.eq_ignore_ascii_case(doc_type_prefix))?;
    let type_dir = root.join(&type_def.dir);

    let new_id = match type_def.numbering {
        NumberingStrategy::Sqids => {
            let sqids_config = config.documents.sqids.as_ref()?;
            let sqid = next_sqids_id(&type_dir, &type_def.prefix, sqids_config).ok()?;
            type_def.make_id(&sqid)
        }
        NumberingStrategy::Incremental => {
            let new_num = next_number(&type_dir, &type_def.prefix);
            type_def.make_id(format_args!("{:03}", new_num))
        }
        NumberingStrategy::Reserved => {
            return None;
        }
    };

    let filename = doc.path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let is_subfolder = filename == "index.md";

    let old_path_str = doc.path.display().to_string();

    if is_subfolder {
        let parent_rel = doc.path.parent()?;
        let parent_name = parent_rel.file_name().and_then(|f| f.to_str())?;
        let new_dir_name = parent_name.replacen(old_id, &new_id, 1);
        let new_parent_rel = parent_rel.with_file_name(&new_dir_name);
        let new_path_str = new_parent_rel.join("index.md").display().to_string();

        let old_abs = root.join(parent_rel);
        let new_abs = root.join(&new_parent_rel);

        if !dry_run {
            fs.rename(&old_abs, &new_abs).ok()?;
            update_title_in_file(&new_abs.join("index.md"), old_id, &new_id, fs);
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
        let stem = doc.path.file_stem().and_then(|f| f.to_str())?;
        let new_stem = stem.replacen(old_id, &new_id, 1);
        let new_filename = format!("{}.md", new_stem);
        let new_rel = doc.path.with_file_name(&new_filename);
        let new_path_str = new_rel.display().to_string();

        let old_abs = root.join(&doc.path);
        let new_abs = root.join(&new_rel);

        if !dry_run {
            fs.rename(&old_abs, &new_abs).ok()?;
            update_title_in_file(&new_abs, old_id, &new_id, fs);
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
