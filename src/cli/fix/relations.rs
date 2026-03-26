use std::collections::HashMap;
use std::path::Path;

use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use super::RelationFixResult;

fn is_path_target(target: &str) -> bool {
    target.contains('/') || target.ends_with(".md")
}

pub(super) fn collect_relation_fixes(
    root: &Path,
    store: &Store,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<RelationFixResult> {
    // Build path -> ID lookup from store docs
    let path_to_id: HashMap<String, String> = store
        .all_docs()
        .iter()
        .map(|doc| (doc.path.to_string_lossy().to_string(), doc.id.clone()))
        .collect();

    let mut results = Vec::new();

    for doc in store.all_docs() {
        if doc.virtual_doc {
            continue;
        }

        // Check if any related targets look like paths
        let path_targets: Vec<(String, String)> = doc
            .related
            .iter()
            .filter(|rel| is_path_target(&rel.target))
            .filter_map(|rel| {
                path_to_id
                    .get(&rel.target)
                    .map(|id| (rel.target.clone(), id.clone()))
            })
            .collect();

        if path_targets.is_empty() {
            continue;
        }

        let full_path = root.join(&doc.path);
        let written = if !dry_run {
            let targets = path_targets.clone();
            let res = rewrite_frontmatter(&full_path, fs, |value| {
                if let Some(related_seq) = value
                    .get_mut("related")
                    .and_then(|v| v.as_sequence_mut())
                {
                    for entry in related_seq.iter_mut() {
                        if let Some(mapping) = entry.as_mapping_mut() {
                            for (_key, val) in mapping.iter_mut() {
                                let replacement = val.as_str().and_then(|s| {
                                    targets.iter().find_map(|(old_path, new_id)| {
                                        if s == old_path {
                                            Some(new_id.clone())
                                        } else {
                                            None
                                        }
                                    })
                                });
                                if let Some(new_val) = replacement {
                                    *val = serde_yaml::Value::String(new_val);
                                }
                            }
                        }
                    }
                }
                Ok(())
            });
            res.is_ok()
        } else {
            false
        };

        let replacements: Vec<(String, String)> = path_targets;

        results.push(RelationFixResult {
            path: doc.path.display().to_string(),
            replacements,
            written,
        });
    }

    results
}
