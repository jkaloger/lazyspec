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
        match push::describe_error(r) {
            Some(message) => eprintln!("error: {message}"),
            None => {
                let branch = r.branch.as_deref().unwrap_or("default branch");
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
    if r.error.is_some() {
        entry["error"] = error_json(r);
    }
    entry
}

// The `message` field is `describe_error`'s wording (the human line uses the
// same), so the two surfaces never drift; `files`/`collisions` stay
// structured here for a caller that wants to act on them programmatically.
fn error_json(result: &CloneResult) -> serde_json::Value {
    let message = push::describe_error(result).unwrap_or_default();
    match result
        .error
        .as_ref()
        .expect("error_json called without an error")
    {
        CloneError::RebaseConflict(conflict) => serde_json::json!({
            "kind": "rebase_conflict",
            "message": message,
            "files": conflict.files,
        }),
        CloneError::RebaseInProgress(_) => serde_json::json!({
            "kind": "rebase_in_progress",
            "message": message,
        }),
        CloneError::UncommittedChanges(_) => serde_json::json!({
            "kind": "uncommitted_changes",
            "message": message,
        }),
        CloneError::DuplicateIds { collisions, .. } => serde_json::json!({
            "kind": "duplicate_ids",
            "message": message,
            "collisions": collisions
                .iter()
                .map(|c| serde_json::json!({ "id": c.id, "paths": c.paths }))
                .collect::<Vec<_>>(),
        }),
        CloneError::Other(_) => serde_json::json!({
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
        let clone = std::path::PathBuf::from("/proj/.lazyspec/git/x");
        let result = CloneResult {
            path: clone.clone(),
            remote: REMOTE.to_string(),
            branch: Some("next".to_string()),
            types: vec!["rfc".to_string()],
            pushed: 0,
            error: Some(CloneError::DuplicateIds {
                clone: clone.clone(),
                collisions: vec![push::DuplicateId {
                    id: "RFC-002".to_string(),
                    paths: vec![
                        "docs/rfcs/RFC-002-a.md".to_string(),
                        "docs/rfcs/RFC-002-b.md".to_string(),
                    ],
                }],
            }),
        };

        let value = error_json(&result);

        assert_eq!(value["kind"], "duplicate_ids");
        assert!(value["message"]
            .as_str()
            .unwrap()
            .contains("/proj/.lazyspec/git/x"));
        assert_eq!(value["collisions"][0]["id"], "RFC-002");
        assert_eq!(value["collisions"][0]["paths"].as_array().unwrap().len(), 2);
    }
}
