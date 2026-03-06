use crate::engine::document::split_frontmatter;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn ignore(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;
    let (yaml, body) = split_frontmatter(&content)?;

    let mut doc: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
    doc["validate-ignore"] = serde_yaml::Value::Bool(true);

    let new_yaml = serde_yaml::to_string(&doc)?;
    let new_content = format!("---\n{}---\n{}", new_yaml, body);
    fs::write(&full_path, new_content)?;

    Ok(())
}

pub fn unignore(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;
    let (yaml, body) = split_frontmatter(&content)?;

    let mut doc: serde_yaml::Value = serde_yaml::from_str(&yaml)?;

    if let Some(mapping) = doc.as_mapping_mut() {
        mapping.remove(&serde_yaml::Value::String("validate-ignore".to_string()));
    }

    let new_yaml = serde_yaml::to_string(&doc)?;
    let new_content = format!("---\n{}---\n{}", new_yaml, body);
    fs::write(&full_path, new_content)?;

    Ok(())
}
