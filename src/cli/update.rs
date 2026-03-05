use crate::engine::document::split_frontmatter;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, doc_path: &str, updates: &[(&str, &str)]) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter(&content)?;

    let mut doc: serde_yaml::Value = serde_yaml::from_str(&yaml)?;

    for (key, value) in updates {
        doc[*key] = serde_yaml::Value::String(value.to_string());
    }

    let new_yaml = serde_yaml::to_string(&doc)?;
    let new_content = format!("---\n{}---\n{}", new_yaml, body);

    fs::write(&full_path, new_content)?;
    Ok(())
}
