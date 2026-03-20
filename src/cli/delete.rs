use crate::cli::resolve::resolve_to_path;
use crate::engine::store::Store;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, store: &Store, doc_path: &str) -> Result<()> {
    let resolved = resolve_to_path(store, doc_path)?;
    let full_path = root.join(&resolved);
    if !full_path.exists() {
        return Err(anyhow::anyhow!("file not found: {}", resolved.display()));
    }
    fs::remove_file(&full_path)?;
    Ok(())
}
