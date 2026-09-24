//! `lazyspec push` (BUG-032 AC3/AC4/AC6): publishes what a `git`-store write
//! already committed locally.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::engine::config::{Config, StoreBackend};
use crate::engine::git_ref::{GitRefOps, RebaseConflict, RebaseInProgress};
use crate::engine::git_store::type_clone_root;
use crate::engine::store::extract_id;

/// A git type sharing a clone, reduced to what the duplicate-id check and the
/// CLI surface need from it: its name and its `dir` relative to the clone
/// root.
#[derive(Debug, Clone)]
pub struct ClonedTypeInfo {
    pub name: String,
    pub dir: String,
}

/// One shared clone among the configured `git` types (BUG-032 AC1), whether
/// or not it has been cloned to disk yet -- [`exists`](CloneGroup::exists)
/// tells a caller that.
#[derive(Debug, Clone)]
pub struct CloneGroup {
    pub remote: String,
    pub branch: Option<String>,
    pub path: PathBuf,
    pub types: Vec<ClonedTypeInfo>,
}

impl CloneGroup {
    pub fn type_names(&self) -> Vec<String> {
        self.types.iter().map(|t| t.name.clone()).collect()
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

/// Every distinct clone the configured `git` types resolve to, deduped by
/// clone root so two types sharing a remote + branch appear once (BUG-032
/// AC1). Order follows first appearance in `[[types]]`.
pub fn distinct_clones(root: &Path, config: &Config) -> Vec<CloneGroup> {
    let mut groups: Vec<CloneGroup> = Vec::new();
    for type_def in config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::Git)
    {
        let path = type_clone_root(root, type_def);
        let info = ClonedTypeInfo {
            name: type_def.name.clone(),
            dir: type_def.dir.clone(),
        };
        match groups.iter_mut().find(|g| g.path == path) {
            Some(g) => g.types.push(info),
            None => groups.push(CloneGroup {
                remote: type_def
                    .remote
                    .clone()
                    .expect("Config::parse rejects a git store without a remote"),
                branch: type_def.branch.clone(),
                path,
                types: vec![info],
            }),
        }
    }
    groups
}

/// One doc id claimed by more than one path in a type's directory -- the
/// human renames one and re-runs `push` (AC6); nothing here guesses which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateId {
    pub id: String,
    pub paths: Vec<String>,
}

/// Why a clone's push did not land.
#[derive(Debug)]
pub enum CloneError {
    RebaseConflict(RebaseConflict),
    RebaseInProgress(RebaseInProgress),
    DuplicateIds {
        clone: PathBuf,
        collisions: Vec<DuplicateId>,
    },
    Other(String),
}

/// One clone's push attempt: `types` is every git type sharing it, `pushed`
/// is how many commits landed, and `error` is `None` only when they all did.
#[derive(Debug)]
pub struct CloneResult {
    pub path: PathBuf,
    pub remote: String,
    pub branch: Option<String>,
    pub types: Vec<String>,
    pub pushed: usize,
    pub error: Option<CloneError>,
}

/// A `CloneResult`'s error, in the wording `lazyspec push`'s human/`--json`
/// surfaces use (`src/cli/push.rs`): the clone path always named, plus --
/// for `RebaseConflict`/`RebaseInProgress`/`DuplicateIds` -- the commands to
/// resolve it there. Lives here, not in `cli`, so a caller that must not
/// depend on the CLI (the TUI, BUG-032 AC7) still surfaces the same content.
pub fn describe_error(result: &CloneResult) -> Option<String> {
    let err = result.error.as_ref()?;
    Some(match err {
        CloneError::RebaseConflict(conflict) => conflict.to_string(),
        CloneError::RebaseInProgress(in_progress) => in_progress.to_string(),
        CloneError::DuplicateIds { clone, collisions } => {
            let pairs: Vec<String> = collisions
                .iter()
                .map(|c| format!("{} -> {}", c.id, c.paths.join(", ")))
                .collect();
            format!(
                "duplicate ids in {}: {}; rename one of each pair, then `lazyspec push`",
                clone.display(),
                pairs.join(", ")
            )
        }
        CloneError::Other(message) => {
            let branch = result.branch.as_deref().unwrap_or("default branch");
            format!(
                "{} ({}, {}): {}",
                result.path.display(),
                result.remote,
                branch,
                message
            )
        }
    })
}

/// Pushes every existing shared clone among the configured `git` types
/// (BUG-032 AC3/AC4/AC6); one clone failing does not stop the rest.
pub fn run(root: &Path, config: &Config, ops: &dyn GitRefOps) -> Vec<CloneResult> {
    distinct_clones(root, config)
        .into_iter()
        .filter(CloneGroup::exists)
        .map(|group| push_one(ops, group))
        .collect()
}

fn push_one(ops: &dyn GitRefOps, group: CloneGroup) -> CloneResult {
    let mut result = CloneResult {
        path: group.path.clone(),
        remote: group.remote.clone(),
        branch: group.branch.clone(),
        types: group.type_names(),
        pushed: 0,
        error: None,
    };

    if let Err(err) = ops.rebase_onto_remote(&group.path, group.branch.as_deref()) {
        result.error = Some(classify_rebase_error(err));
        return result;
    }

    match duplicate_ids(ops, &group) {
        Ok(collisions) if !collisions.is_empty() => {
            result.error = Some(CloneError::DuplicateIds {
                clone: group.path.clone(),
                collisions,
            });
            return result;
        }
        Ok(_) => {}
        Err(err) => {
            result.error = Some(CloneError::Other(format!("{err:#}")));
            return result;
        }
    }

    match ops.push(&group.path, group.branch.as_deref()) {
        Ok(pushed) => result.pushed = pushed,
        Err(err) => result.error = Some(CloneError::Other(format!("{err:#}"))),
    }

    result
}

fn classify_rebase_error(err: anyhow::Error) -> CloneError {
    let err = match err.downcast::<RebaseConflict>() {
        Ok(conflict) => return CloneError::RebaseConflict(conflict),
        Err(err) => err,
    };
    match err.downcast::<RebaseInProgress>() {
        Ok(in_progress) => CloneError::RebaseInProgress(in_progress),
        Err(err) => CloneError::Other(format!("{err:#}")),
    }
}

/// Any doc added in `origin/<branch>..HEAD` whose id now collides with
/// another doc of the same type in the clone (BUG-032 AC6). The check itself
/// is scoped per type -- an id is only ever compared against its own type's
/// directory -- but a collision still blocks the whole clone's push, because
/// two types sharing a clone share its one push. `reserved` numbering claims
/// its id remotely at create time, so it never reaches here with a collision
/// to report; every other numbering strategy is checked the same way.
fn duplicate_ids(ops: &dyn GitRefOps, group: &CloneGroup) -> anyhow::Result<Vec<DuplicateId>> {
    let added = ops.added_files(&group.path, group.branch.as_deref())?;
    let mut collisions: Vec<DuplicateId> = Vec::new();
    for type_info in &group.types {
        let type_dir = group.path.join(&type_info.dir);
        let existing = docs_by_id(&group.path, &type_dir);
        for added_path in &added {
            let full = group.path.join(added_path);
            if !full.starts_with(&type_dir) {
                continue;
            }
            let id = extract_id(&full);
            if collisions.iter().any(|c| c.id == id) {
                continue;
            }
            if let Some(paths) = existing.get(&id) {
                if paths.len() > 1 {
                    collisions.push(DuplicateId {
                        id,
                        paths: paths.clone(),
                    });
                }
            }
        }
    }
    Ok(collisions)
}

/// Every `.md` file under `dir`, grouped by [`extract_id`], as paths relative
/// to `clone` -- what the CLI surface names a collision by.
fn docs_by_id(clone: &Path, dir: &Path) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in walk_md_files(dir) {
        let id = extract_id(&path);
        let rel = path
            .strip_prefix(clone)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        map.entry(id).or_default().push(rel);
    }
    for paths in map.values_mut() {
        paths.sort();
    }
    map
}

fn walk_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_md_files(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{StoreBackend, TypeDef};
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use tempfile::TempDir;

    const REMOTE: &str = "https://example.com/specs.git";

    fn git_type(name: &str, dir: &str, remote: &str, branch: Option<&str>) -> TypeDef {
        TypeDef {
            dir: dir.to_string(),
            remote: Some(remote.to_string()),
            branch: branch.map(str::to_string),
            ..TypeDef::test_fixture(name, StoreBackend::Git)
        }
    }

    fn config_with(types: Vec<TypeDef>) -> Config {
        let mut config = Config::default();
        config.documents.types = types;
        config
    }

    fn write_doc(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "doc").unwrap();
    }

    #[test]
    fn distinct_clones_dedupes_two_types_on_one_remote_and_branch() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![
            git_type("rfc", "docs/rfcs", REMOTE, Some("next")),
            git_type("spec", "docs/specs", REMOTE, Some("next")),
        ]);

        let clones = distinct_clones(tmp.path(), &config);

        assert_eq!(clones.len(), 1);
        assert_eq!(clones[0].type_names(), vec!["rfc", "spec"]);
    }

    #[test]
    fn distinct_clones_keeps_a_different_remote_separate() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![
            git_type("rfc", "docs/rfcs", REMOTE, Some("next")),
            git_type(
                "spec",
                "docs/specs",
                "https://example.com/other.git",
                Some("next"),
            ),
        ]);

        let clones = distinct_clones(tmp.path(), &config);

        assert_eq!(clones.len(), 2);
    }

    #[test]
    fn run_skips_a_clone_that_was_never_cloned_to_disk() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![git_type("rfc", "docs/rfcs", REMOTE, Some("next"))]);

        let results = run(tmp.path(), &config, &MockGitRefClient::new());

        assert!(results.is_empty());
    }

    #[test]
    fn run_visits_every_clone_even_when_one_fails() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![
            git_type("rfc", "docs/rfcs", REMOTE, Some("next")),
            git_type(
                "spec",
                "docs/specs",
                "https://example.com/other.git",
                Some("next"),
            ),
        ]);
        for clone in distinct_clones(tmp.path(), &config) {
            std::fs::create_dir_all(&clone.path).unwrap();
        }

        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Err(anyhow::anyhow!("network unreachable")))
            .with_rebase_onto_remote_result(Ok(()))
            .with_added_files_result(Ok(vec![]))
            .with_push_commits_result(Ok(2));

        let results = run(tmp.path(), &config, &ops);

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .any(|r| matches!(r.error, Some(CloneError::Other(_)))),
            "{results:?}"
        );
        assert!(
            results.iter().any(|r| r.error.is_none() && r.pushed == 2),
            "the other clone's push still ran: {results:?}"
        );
    }

    #[test]
    fn a_rebase_conflict_is_reported_and_nothing_is_pushed() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![git_type("rfc", "docs/rfcs", REMOTE, Some("next"))]);
        let clone = distinct_clones(tmp.path(), &config)[0].path.clone();
        std::fs::create_dir_all(&clone).unwrap();

        let conflict = RebaseConflict {
            clone: clone.clone(),
            files: vec!["docs/rfcs/RFC-001-a.md".to_string()],
        };
        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Err(anyhow::Error::new(conflict)));

        let results = run(tmp.path(), &config, &ops);

        assert_eq!(results.len(), 1);
        match &results[0].error {
            Some(CloneError::RebaseConflict(c)) => {
                assert_eq!(c.clone, clone);
                assert_eq!(c.files, vec!["docs/rfcs/RFC-001-a.md".to_string()]);
            }
            other => panic!("expected a rebase conflict, got {other:?}"),
        }
        assert_eq!(results[0].pushed, 0);
    }

    #[test]
    fn a_duplicate_id_between_an_added_doc_and_an_existing_one_blocks_the_push() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![git_type("rfc", "docs/rfcs", REMOTE, Some("next"))]);
        let clone = distinct_clones(tmp.path(), &config)[0].path.clone();
        write_doc(&clone, "docs/rfcs/RFC-002-mine.md");
        write_doc(&clone, "docs/rfcs/RFC-002-theirs.md");

        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Ok(()))
            .with_added_files_result(Ok(vec!["docs/rfcs/RFC-002-mine.md".to_string()]));

        let results = run(tmp.path(), &config, &ops);

        assert_eq!(results.len(), 1);
        match &results[0].error {
            Some(CloneError::DuplicateIds {
                clone: c,
                collisions,
            }) => {
                assert_eq!(c, &clone);
                assert_eq!(collisions.len(), 1);
                assert_eq!(collisions[0].id, "RFC-002");
                let mut paths = collisions[0].paths.clone();
                paths.sort();
                assert_eq!(
                    paths,
                    vec![
                        "docs/rfcs/RFC-002-mine.md".to_string(),
                        "docs/rfcs/RFC-002-theirs.md".to_string(),
                    ]
                );
            }
            other => panic!("expected duplicate ids, got {other:?}"),
        }
        assert_eq!(results[0].pushed, 0);
    }

    #[test]
    fn describe_error_names_the_clone_path_for_a_rebase_conflict() {
        let clone = PathBuf::from("/proj/.lazyspec/git/x");
        let result = CloneResult {
            path: clone.clone(),
            remote: REMOTE.to_string(),
            branch: Some("next".to_string()),
            types: vec!["rfc".to_string()],
            pushed: 0,
            error: Some(CloneError::RebaseConflict(RebaseConflict {
                clone: clone.clone(),
                files: vec!["docs/rfcs/RFC-001-a.md".to_string()],
            })),
        };

        let message = describe_error(&result).unwrap();

        assert!(message.contains(&clone.display().to_string()), "{message}");
        assert!(message.contains("pull --rebase"), "{message}");
    }

    #[test]
    fn describe_error_names_the_clone_path_for_duplicate_ids() {
        let clone = PathBuf::from("/proj/.lazyspec/git/x");
        let result = CloneResult {
            path: clone.clone(),
            remote: REMOTE.to_string(),
            branch: Some("next".to_string()),
            types: vec!["rfc".to_string()],
            pushed: 0,
            error: Some(CloneError::DuplicateIds {
                clone: clone.clone(),
                collisions: vec![DuplicateId {
                    id: "RFC-002".to_string(),
                    paths: vec![
                        "docs/rfcs/RFC-002-a.md".to_string(),
                        "docs/rfcs/RFC-002-b.md".to_string(),
                    ],
                }],
            }),
        };

        let message = describe_error(&result).unwrap();

        assert!(message.contains(&clone.display().to_string()), "{message}");
        assert!(message.contains("RFC-002"), "{message}");
        assert!(message.contains("lazyspec push"), "{message}");
    }

    #[test]
    fn describe_error_is_none_when_the_clone_pushed_cleanly() {
        let result = CloneResult {
            path: PathBuf::from("/proj/.lazyspec/git/x"),
            remote: REMOTE.to_string(),
            branch: Some("next".to_string()),
            types: vec!["rfc".to_string()],
            pushed: 2,
            error: None,
        };

        assert_eq!(describe_error(&result), None);
    }

    #[test]
    fn no_collision_pushes_normally() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![git_type("rfc", "docs/rfcs", REMOTE, Some("next"))]);
        let clone = distinct_clones(tmp.path(), &config)[0].path.clone();
        write_doc(&clone, "docs/rfcs/RFC-002-mine.md");

        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Ok(()))
            .with_added_files_result(Ok(vec!["docs/rfcs/RFC-002-mine.md".to_string()]))
            .with_push_commits_result(Ok(1));

        let results = run(tmp.path(), &config, &ops);

        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
        assert_eq!(results[0].pushed, 1);
    }

    // Two types sharing a clone push (or don't) together -- a collision in
    // one type's directory blocks the whole clone, the sibling type's own
    // added doc included, because there is one push per clone, not per type.
    #[test]
    fn a_collision_in_one_type_blocks_the_whole_shared_clones_push() {
        let tmp = TempDir::new().unwrap();
        let config = config_with(vec![
            git_type("rfc", "docs/rfcs", REMOTE, Some("next")),
            git_type("spec", "docs/specs", REMOTE, Some("next")),
        ]);
        let clone = distinct_clones(tmp.path(), &config)[0].path.clone();
        write_doc(&clone, "docs/rfcs/RFC-002-mine.md");
        write_doc(&clone, "docs/rfcs/RFC-002-theirs.md");
        write_doc(&clone, "docs/specs/SPEC-001-mine.md");

        let ops = MockGitRefClient::new()
            .with_rebase_onto_remote_result(Ok(()))
            .with_added_files_result(Ok(vec![
                "docs/rfcs/RFC-002-mine.md".to_string(),
                "docs/specs/SPEC-001-mine.md".to_string(),
            ]));

        let results = run(tmp.path(), &config, &ops);

        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].error,
            Some(CloneError::DuplicateIds { .. })
        ));
    }
}
