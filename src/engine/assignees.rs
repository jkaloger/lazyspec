use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store_dispatch::{DocumentStore, FilesystemStore, GithubIssuesStore};
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn set_assignees(
    root: &Path,
    config: &Config,
    type_name: &str,
    doc_id: &str,
    new_list: &[String],
) -> Result<()> {
    let type_def = config
        .type_by_name(type_name)
        .ok_or_else(|| anyhow!("unknown document type: {}", type_name))?;

    match type_def.store {
        StoreBackend::Filesystem => {
            let mut fs_store = FilesystemStore {
                root: root.to_path_buf(),
                config: config.clone(),
            };
            fs_store.set_assignees(type_def, doc_id, new_list)
        }
        StoreBackend::GithubIssues => {
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
            gh_store.set_assignees(type_def, doc_id, new_list)
        }
        StoreBackend::GitRef => {
            let mut git_store = GitRefStore {
                git: GitCli,
                root: root.to_path_buf(),
                config: config.clone(),
                reserved_number: None,
            };
            git_store.set_assignees(type_def, doc_id, new_list)
        }
    }
}
