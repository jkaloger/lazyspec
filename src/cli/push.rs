//! `lazyspec push` (BUG-032 AC3/AC4/AC6): the CLI surface over
//! [`crate::engine::ops::push`]. Rebases, checks for a duplicate id, then
//! pushes every existing shared git-store clone; one clone's failure never
//! stops the rest, and the process exits non-zero when any clone errored.

use crate::engine::config::Config;
use crate::engine::git_ref::GitRefOps;
use crate::engine::ops::push::{self, CloneError, CloneResult};
use anyhow::{bail, Result};
use std::path::Path;

pub fn run(root: &Path, config: &Config, ops: &dyn GitRefOps, json: bool) -> Result<()> {
    let results = push::run(root, config, ops);

    if json {
        println!("{}", results_json(&results));
    } else {
        print_human(&results);
    }

    if results.iter().any(|r| r.error.is_some()) {
        bail!("push failed for one or more clones");
    }
    Ok(())
}

fn print_human(results: &[CloneResult]) {
    if results.is_empty() {
        println!("No git clones to push.");
        return;
    }

    for r in results {
        let branch = r.branch.as_deref().unwrap_or("default branch");
        match &r.error {
            None => {
                let status = if r.pushed == 0 {
                    "up to date".to_string()
                } else {
                    format!(
                        "{} commit{} pushed",
                        r.pushed,
                        if r.pushed == 1 { "" } else { "s" }
                    )
                };
                println!(
                    "{} ({}, {}): {}",
                    r.path.display(),
                    r.remote,
                    branch,
                    status
                );
            }
            Some(CloneError::RebaseConflict(conflict)) => {
                eprintln!("error: {}", conflict);
            }
            Some(CloneError::RebaseInProgress(in_progress)) => {
                eprintln!("error: {}", in_progress);
            }
            Some(CloneError::DuplicateIds { clone, collisions }) => {
                eprintln!("error: duplicate ids in {}:", clone.display());
                for c in collisions {
                    eprintln!("  {} -> {}", c.id, c.paths.join(", "));
                }
                eprintln!("rename one of each pair, then `lazyspec push`");
            }
            Some(CloneError::Other(message)) => {
                eprintln!(
                    "error: {} ({}, {}): {}",
                    r.path.display(),
                    r.remote,
                    branch,
                    message
                );
            }
        }
    }
}

fn results_json(results: &[CloneResult]) -> String {
    let clones: Vec<serde_json::Value> = results.iter().map(clone_json).collect();
    serde_json::to_string_pretty(&serde_json::json!({ "clones": clones })).unwrap()
}

fn clone_json(r: &CloneResult) -> serde_json::Value {
    let mut entry = serde_json::json!({
        "path": r.path.display().to_string(),
        "remote": r.remote,
        "branch": r.branch,
        "types": r.types,
        "pushed": r.pushed,
        "error": serde_json::Value::Null,
    });
    if let Some(err) = &r.error {
        entry["error"] = error_json(err);
    }
    entry
}

fn error_json(err: &CloneError) -> serde_json::Value {
    match err {
        CloneError::RebaseConflict(conflict) => serde_json::json!({
            "kind": "rebase_conflict",
            "message": conflict.to_string(),
            "files": conflict.files,
        }),
        CloneError::RebaseInProgress(in_progress) => serde_json::json!({
            "kind": "rebase_in_progress",
            "message": in_progress.to_string(),
        }),
        CloneError::DuplicateIds { clone, collisions } => serde_json::json!({
            "kind": "duplicate_ids",
            "message": format!(
                "duplicate ids in {}; rename one of each pair, then `lazyspec push`",
                clone.display()
            ),
            "collisions": collisions
                .iter()
                .map(|c| serde_json::json!({ "id": c.id, "paths": c.paths }))
                .collect::<Vec<_>>(),
        }),
        CloneError::Other(message) => serde_json::json!({
            "kind": "other",
            "message": message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{StoreBackend, TypeDef};
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use crate::engine::git_ref::RebaseConflict;
    use crate::engine::ops::push::distinct_clones;
    use tempfile::TempDir;

    const REMOTE: &str = "https://example.com/specs.git";

    fn git_config(branch: Option<&str>) -> Config {
        let mut config = Config::default();
        config.documents.types = vec![TypeDef {
            dir: "docs/rfcs".to_string(),
            remote: Some(REMOTE.to_string()),
            branch: branch.map(str::to_string),
            ..TypeDef::test_fixture("rfc", StoreBackend::Git)
        }];
        config
    }

    #[test]
    fn no_git_types_configured_exits_zero_with_an_empty_list() {
        let tmp = TempDir::new().unwrap();
        let config = Config::default();

        let result = run(tmp.path(), &config, &MockGitRefClient::new(), true);

        assert!(result.is_ok());
    }

    #[test]
    fn a_clean_push_exits_zero() {
        let tmp = TempDir::new().unwrap();
        let config = git_config(Some("next"));
        let clone = distinct_clones(tmp.path(), &config)[0].path.clone();
        std::fs::create_dir_all(&clone).unwrap();

        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Ok(()))
            .with_added_files_result(Ok(vec![]))
            .with_push_commits_result(Ok(3));

        let result = run(tmp.path(), &config, &ops, true);

        assert!(result.is_ok());
    }

    #[test]
    fn a_rebase_conflict_exits_non_zero() {
        let tmp = TempDir::new().unwrap();
        let config = git_config(Some("next"));
        let clone = distinct_clones(tmp.path(), &config)[0].path.clone();
        std::fs::create_dir_all(&clone).unwrap();

        let conflict = RebaseConflict {
            clone: clone.clone(),
            files: vec!["docs/rfcs/RFC-001-a.md".to_string()],
        };
        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Err(anyhow::Error::new(conflict)));

        let result = run(tmp.path(), &config, &ops, false);

        assert!(result.is_err());
    }

    #[test]
    fn error_json_names_the_clone_and_collisions_for_duplicate_ids() {
        let value = error_json(&CloneError::DuplicateIds {
            clone: std::path::PathBuf::from("/proj/.lazyspec/git/x"),
            collisions: vec![push::DuplicateId {
                id: "RFC-002".to_string(),
                paths: vec![
                    "docs/rfcs/RFC-002-a.md".to_string(),
                    "docs/rfcs/RFC-002-b.md".to_string(),
                ],
            }],
        });

        assert_eq!(value["kind"], "duplicate_ids");
        assert!(value["message"]
            .as_str()
            .unwrap()
            .contains("/proj/.lazyspec/git/x"));
        assert_eq!(value["collisions"][0]["id"], "RFC-002");
        assert_eq!(value["collisions"][0]["paths"].as_array().unwrap().len(), 2);
    }
}
