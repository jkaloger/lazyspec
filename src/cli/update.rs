use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, doc_path: &str, updates: &[(&str, &str)]) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter_raw(&content)?;

    let mut doc: serde_yaml::Value = serde_yaml::from_str(&yaml)?;

    for (key, value) in updates {
        doc[*key] = serde_yaml::Value::String(value.to_string());
    }

    let new_yaml = serde_yaml::to_string(&doc)?;
    let new_content = format!("---\n{}---\n{}", new_yaml, body);

    fs::write(&full_path, new_content)?;
    Ok(())
}

fn split_frontmatter_raw(content: &str) -> Result<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(anyhow::anyhow!("no frontmatter"));
    }
    let after = &trimmed[3..];
    let end = after
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("unterminated frontmatter"))?;
    let yaml = after[..end].to_string();
    let body = after[end + 4..].to_string();
    Ok((yaml, body))
}
