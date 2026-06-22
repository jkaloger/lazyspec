use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::fs_ops;
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use anyhow::{anyhow, bail, Result};
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
            if let Some((_, target)) = updates.iter().find(|(k, _)| *k == "status") {
                let current = doc.status.as_str();
                if current != *target && !type_def.lifecycle.has_edge(current, target) {
                    let allowed = type_def.lifecycle.targets_from(current);
                    let allowed = if allowed.is_empty() {
                        "(none)".to_string()
                    } else {
                        allowed.join(", ")
                    };
                    bail!(
                        "invalid transition for type \"{}\": no edge from \"{}\" to \"{}\" (allowed targets: {})",
                        type_name,
                        current,
                        target,
                        allowed
                    );
                }
            }
            if type_def.store == StoreBackend::GithubIssues {
                let gh_config = config.documents.github.as_ref().ok_or_else(|| {
                    anyhow!(
                        "type '{}' uses github-issues store but no [github] config found",
                        type_name
                    )
                })?;
                let repo = gh_config.repo.as_ref().ok_or_else(|| {
                    anyhow!(
                        "type '{}' uses github-issues store but no github.repo configured",
                        type_name
                    )
                })?;
                let mut gh_store = GithubIssuesStore {
                    client: GhCli::new(),
                    root: root.to_path_buf(),
                    repo: repo.clone(),
                    config: config.clone(),
                    issue_map: IssueMap::load(root)?,
                    issue_cache: IssueCache::new(root),
                };
                return gh_store.update(type_def, &doc.id, updates);
            }
            if type_def.store == StoreBackend::GitRef {
                let mut git_store = GitRefStore {
                    git: GitCli,
                    root: root.to_path_buf(),
                    config: config.clone(),
                    reserved_number: None,
                };
                return git_store.update(type_def, &doc.id, updates);
            }
        }
    }

    fs_ops::update_document(root, store, doc_path, updates)
}
