use crate::cli::resolve::resolve_to_path;
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn ignore(root: &Path, store: &Store, doc_path: &str, fs: &dyn FileSystem) -> Result<()> {
    let resolved = resolve_to_path(store, doc_path)?;
    let full_path = root.join(&resolved);
    rewrite_frontmatter(&full_path, fs, |doc| {
        doc["validate-ignore"] = serde_yaml::Value::Bool(true);
        Ok(())
    })
}

pub fn unignore(root: &Path, store: &Store, doc_path: &str, fs: &dyn FileSystem) -> Result<()> {
    let resolved = resolve_to_path(store, doc_path)?;
    let full_path = root.join(&resolved);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if let Some(mapping) = doc.as_mapping_mut() {
            mapping.remove(&serde_yaml::Value::String("validate-ignore".to_string()));
        }
        Ok(())
    })
}
