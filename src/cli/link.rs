use crate::engine::document::rewrite_frontmatter;
use anyhow::Result;
use std::path::Path;

pub fn link(root: &Path, from: &str, rel_type: &str, to: &str) -> Result<()> {
    let full_path = root.join(from);
    rewrite_frontmatter(&full_path, |doc| {
        if doc.get("related").is_none() {
            doc["related"] = serde_yaml::Value::Sequence(vec![]);
        }
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::String(rel_type.to_string()),
            serde_yaml::Value::String(to.to_string()),
        );
        doc["related"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::Value::Mapping(entry));
        Ok(())
    })
}

pub fn unlink(root: &Path, from: &str, rel_type: &str, to: &str) -> Result<()> {
    let full_path = root.join(from);
    rewrite_frontmatter(&full_path, |doc| {
        if let Some(related) = doc.get_mut("related").and_then(|r| r.as_sequence_mut()) {
            related.retain(|entry| {
                if let Some(map) = entry.as_mapping() {
                    let key = serde_yaml::Value::String(rel_type.to_string());
                    if let Some(val) = map.get(&key) {
                        return val.as_str() != Some(to);
                    }
                }
                true
            });
        }
        Ok(())
    })
}
