use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store_dispatch::{DocumentStore, FilesystemStore, GithubIssuesStore};
use anyhow::{anyhow, Result};
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum ProvenanceError {
    Empty,
    Duplicate(String),
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProvenanceError::Empty => write!(f, "citation must not be empty"),
            ProvenanceError::Duplicate(s) => write!(f, "citation already present: {}", s),
        }
    }
}

impl std::error::Error for ProvenanceError {}

pub fn validate_citation(citation: &str) -> Result<&str, ProvenanceError> {
    let trimmed = citation.trim();
    if trimmed.is_empty() {
        Err(ProvenanceError::Empty)
    } else {
        Ok(trimmed)
    }
}

pub fn set_provenance(
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
            fs_store.set_provenance(type_def, doc_id, new_list)
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
            gh_store.set_provenance(type_def, doc_id, new_list)
        }
        StoreBackend::GitRef => {
            let mut git_store = GitRefStore {
                git: GitCli,
                root: root.to_path_buf(),
                config: config.clone(),
                reserved_number: None,
            };
            git_store.set_provenance(type_def, doc_id, new_list)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_citation_rejects_empty() {
        assert!(matches!(validate_citation(""), Err(ProvenanceError::Empty)));
    }

    #[test]
    fn validate_citation_rejects_whitespace_only() {
        assert!(matches!(
            validate_citation("   "),
            Err(ProvenanceError::Empty)
        ));
    }

    #[test]
    fn validate_citation_trims_and_returns() {
        assert_eq!(validate_citation("  X  ").unwrap(), "X");
    }

    #[test]
    fn display_empty() {
        assert!(format!("{}", ProvenanceError::Empty).contains("empty"));
    }

    #[test]
    fn display_duplicate() {
        let s = format!("{}", ProvenanceError::Duplicate("foo".into()));
        assert!(s.contains("foo") && s.contains("already"));
    }
}
