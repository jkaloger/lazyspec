use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::GhCli;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn run(root: &Path, store: &Store, doc_path: &str) -> Result<()> {
    run_with_config(root, store, doc_path, None)
}

pub fn run_with_config(
    root: &Path,
    store: &Store,
    doc_path: &str,
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
                let mut gh_store = GithubIssuesStore {
                    client: GhCli::new(),
                    root: root.to_path_buf(),
                    repo: repo.clone(),
                    config: config.clone(),
                    issue_map: IssueMap::load(root)?,
                    issue_cache: IssueCache::new(root),
                };
                return gh_store.delete(type_def, &doc.id);
            }
        }
    }

    crate::engine::fs_ops::delete_document(root, store, doc_path)
}
