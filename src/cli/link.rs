use crate::cli::resolve::{resolve_to_id, resolve_to_path};
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn link(root: &Path, store: &Store, from: &str, rel_type: &str, to: &str, fs: &dyn FileSystem) -> Result<()> {
    let resolved_from = resolve_to_path(store, from)?;
    let to_id = resolve_to_id(store, to)?;
    let full_path = root.join(&resolved_from);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if doc.get("related").is_none() {
            doc["related"] = serde_yaml::Value::Sequence(vec![]);
        }
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::String(rel_type.to_string()),
            serde_yaml::Value::String(to_id.clone()),
        );
        doc["related"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::Value::Mapping(entry));
        Ok(())
    })
}

pub fn unlink(root: &Path, store: &Store, from: &str, rel_type: &str, to: &str, fs: &dyn FileSystem) -> Result<()> {
    let resolved_from = resolve_to_path(store, from)?;
    let to_id = resolve_to_id(store, to)?;
    let full_path = root.join(&resolved_from);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if let Some(related) = doc.get_mut("related").and_then(|r| r.as_sequence_mut()) {
            related.retain(|entry| {
                if let Some(map) = entry.as_mapping() {
                    let key = serde_yaml::Value::String(rel_type.to_string());
                    if let Some(val) = map.get(&key) {
                        return val.as_str() != Some(to_id.as_str());
                    }
                }
                true
            });
        }
        Ok(())
    })
}
