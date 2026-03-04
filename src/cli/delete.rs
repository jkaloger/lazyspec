use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    if !full_path.exists() {
        return Err(anyhow::anyhow!("file not found: {}", doc_path));
    }
    fs::remove_file(&full_path)?;
    Ok(())
}
