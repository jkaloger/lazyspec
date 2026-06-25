use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::fs_ops;
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore, GithubMilestonesStore};
use anyhow::{anyhow, bail, Result};
use std::path::Path;

const RESERVED_ATTR_KEYS: &[&str] = &["status", "title", "body", "author"];

/// Parse repeatable `--attr key=value` flags into owned `(key, value)` pairs.
///
/// Splits on the FIRST `=` so values may themselves contain `=`. A missing `=`,
/// an empty key, or a reserved field name (which has its own dedicated flag) is
/// an error.
pub fn parse_attr_pairs(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|entry| {
            let (key, value) = entry
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid --attr, expected key=value: {entry}"))?;
            if key.is_empty() {
                bail!("invalid --attr, empty key: {entry}");
            }
            if RESERVED_ATTR_KEYS.contains(&key) {
                bail!("'{key}' is a reserved field and cannot be set via --attr; use --{key}");
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

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
            if type_def.store == StoreBackend::GithubMilestones {
                let gh_config = config.documents.github.as_ref().ok_or_else(|| {
                    anyhow!(
                        "type '{}' uses github-milestones store but no [github] config found",
                        type_name
                    )
                })?;
                let repo = gh_config.repo.as_ref().ok_or_else(|| {
                    anyhow!(
                        "type '{}' uses github-milestones store but no github.repo configured",
                        type_name
                    )
                })?;
                let mut ms_store = GithubMilestonesStore {
                    client: GhCli::new(),
                    root: root.to_path_buf(),
                    repo: repo.clone(),
                    config: config.clone(),
                    issue_map: IssueMap::load(root)?,
                };
                return ms_store.update(type_def, &doc.id, updates);
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

            return fs_ops::update_document_with_type(
                root,
                store,
                doc_path,
                updates,
                Some(type_def),
            );
        }
    }

    fs_ops::update_document(root, store, doc_path, updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    // AC4: clap collects each --attr occurrence into a Vec.
    #[test]
    fn clap_collects_multiple_attr_flags() {
        let cli = Cli::try_parse_from([
            "lazyspec",
            "update",
            "STORY-1",
            "--attr",
            "owner=jkaloger",
            "--attr",
            "estimate=3",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Update { attr, .. }) => {
                assert_eq!(attr, vec!["owner=jkaloger", "estimate=3"]);
            }
            _ => panic!("expected Update command"),
        }
    }

    #[test]
    fn parse_attr_pairs_basic() {
        let pairs = parse_attr_pairs(&["owner=jkaloger".to_string()]).unwrap();
        assert_eq!(pairs, vec![("owner".to_string(), "jkaloger".to_string())]);
    }

    // Edge: split on the FIRST '=' so the value may contain '='.
    #[test]
    fn parse_attr_pairs_value_with_equals() {
        let pairs = parse_attr_pairs(&["k=a=b".to_string()]).unwrap();
        assert_eq!(pairs, vec![("k".to_string(), "a=b".to_string())]);
    }

    // Edge: missing '=' bails.
    #[test]
    fn parse_attr_pairs_missing_equals_bails() {
        let err = parse_attr_pairs(&["badpair".to_string()]).unwrap_err();
        assert!(err.to_string().contains("expected key=value"), "got: {err}");
    }

    // Edge: empty key bails.
    #[test]
    fn parse_attr_pairs_empty_key_bails() {
        let err = parse_attr_pairs(&["=v".to_string()]).unwrap_err();
        assert!(err.to_string().contains("empty key"), "got: {err}");
    }

    #[test]
    fn parse_attr_pairs_reserved_key_bails() {
        let err = parse_attr_pairs(&["status=done".to_string()]).unwrap_err();
        assert!(err.to_string().contains("reserved"), "got: {err}");
    }
}
