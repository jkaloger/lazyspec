use crate::cli::resolve::{resolve_shorthand_or_path, resolve_to_path};
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::split_frontmatter;
use crate::engine::gh::GhCli;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use anyhow::{anyhow, Result};
use std::cell::RefCell;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, store: &Store, doc_path: &str, updates: &[(&str, &str)]) -> Result<()> {
    run_with_config(root, store, doc_path, updates, None)
}

pub fn run_with_config(
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
    config: Option<&Config>,
) -> Result<()> {
    if let Some(config) = config {
        let doc = resolve_shorthand_or_path(store, doc_path)?;
        let type_name = doc.doc_type.as_str();
        if let Some(type_def) = config.type_by_name(type_name) {
            if type_def.store == StoreBackend::GithubIssues {
                let gh_config = config.documents.github.as_ref()
                    .ok_or_else(|| anyhow!("type '{}' uses github-issues store but no [github] config found", type_name))?;
                let repo = gh_config.repo.as_ref()
                    .ok_or_else(|| anyhow!("type '{}' uses github-issues store but no github.repo configured", type_name))?;
                let gh_store = GithubIssuesStore {
                    client: GhCli::new(),
                    root: root.to_path_buf(),
                    repo: repo.clone(),
                    config: config.clone(),
                    issue_map: RefCell::new(IssueMap::load(root)?),
                };
                return gh_store.update(type_def, &doc.id, updates);
            }
        }
    }

    let resolved = resolve_to_path(store, doc_path)?;
    let full_path = root.join(&resolved);
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
