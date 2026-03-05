use crate::engine::document::split_frontmatter;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, doc_path: &str, updates: &[(&str, &str)]) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter(&content)?;

    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    for (key, value) in updates {
        let prefix = format!("{}:", key);
        if let Some(line) = lines.iter_mut().find(|l| l.trim_start().starts_with(&prefix)) {
            *line = format!("{}: {}", key, value);
        }
    }

    let new_yaml = lines.join("\n");
    let new_content = format!("---\n{}\n---\n{}", new_yaml, body);
    fs::write(&full_path, new_content)?;
    Ok(())
}
