use crate::engine::document::rewrite_frontmatter;
use anyhow::Result;
use std::path::Path;

pub fn ignore(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    rewrite_frontmatter(&full_path, |doc| {
        doc["validate-ignore"] = serde_yaml::Value::Bool(true);
        Ok(())
    })
}

pub fn unignore(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    rewrite_frontmatter(&full_path, |doc| {
        if let Some(mapping) = doc.as_mapping_mut() {
            mapping.remove(&serde_yaml::Value::String("validate-ignore".to_string()));
        }
        Ok(())
    })
}
