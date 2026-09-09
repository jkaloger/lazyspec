use crate::cli::resolve::{resolve_to_id, resolve_to_path};
use crate::engine::config::Config;
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::git_ref::GitRefOps;
use crate::engine::git_store::commit_if_git_backed;
use crate::engine::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn ignore(
    root: &Path,
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    doc_path: &str,
    fs: &dyn FileSystem,
) -> Result<()> {
    let resolved = resolve_to_path(store, doc_path)?;
    let full_path = root.join(&resolved);
    rewrite_frontmatter(&full_path, fs, |doc| {
        doc["validate-ignore"] = serde_yaml::Value::Bool(true);
        Ok(())
    })?;
    let id = resolve_to_id(store, doc_path)?;
    commit_if_git_backed(root, config, &resolved, git, &format!("ignore {id}"))
}

pub fn unignore(
    root: &Path,
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    doc_path: &str,
    fs: &dyn FileSystem,
) -> Result<()> {
    let resolved = resolve_to_path(store, doc_path)?;
    let full_path = root.join(&resolved);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if let Some(mapping) = doc.as_mapping_mut() {
            mapping.remove(serde_yaml::Value::String("validate-ignore".to_string()));
        }
        Ok(())
    })?;
    let id = resolve_to_id(store, doc_path)?;
    commit_if_git_backed(root, config, &resolved, git, &format!("unignore {id}"))
}
