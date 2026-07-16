use std::path::Path;

use regex::Regex;

use crate::engine::document::{compose_frontmatter, split_frontmatter};
use crate::engine::fs::FileSystem;
use crate::engine::refs::REF_PATTERN;
use crate::engine::store::Store;

use super::ReferenceUpdate;

pub fn cascade_references(
    root: &Path,
    store: &Store,
    old_id: &str,
    new_id: &str,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<ReferenceUpdate> {
    let mut updates = Vec::new();
    let ref_re = Regex::new(REF_PATTERN).unwrap();

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
                            if s.contains(old_id) {
                                let new_val = s.replace(old_id, new_id);
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

        let mut new_body = body.clone();
        let mut body_changed = false;

        for cap in ref_re.captures_iter(&body) {
            let full_match = cap.get(0).unwrap();
            let match_str = full_match.as_str();
            if match_str.contains(old_id) {
                let replaced = match_str.replace(old_id, new_id);
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
            let output = compose_frontmatter(&new_yaml, final_body);
            let _ = fs.write(&full_path, &output);
        }

        updates.extend(file_updates);
    }

    updates
}
