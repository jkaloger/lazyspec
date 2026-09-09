use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::document::DocType;
use crate::engine::fs_ops;
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::git_store::commit_if_git_backed;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::reservation;
use crate::engine::store::{Filter, Store};
use crate::engine::store_dispatch::{
    build_registry, DocumentStore, GithubIssuesStore, GithubMilestonesStore, PushOutcome,
};
use anyhow::{anyhow, bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<PathBuf> {
    run_with_body(
        root,
        config,
        store,
        doc_type,
        title,
        author,
        None,
        None,
        on_progress,
    )
    .map(|(path, _)| path)
}

/// Author a document, returning its path alongside the backend push outcome.
///
/// The outcome is `Synced` for every synchronous backend (filesystem, and the
/// REST/GraphQL stores whose create either lands remotely or errors); only a
/// git-ref-backed create can report `LocalOnly` when the deferred push cannot
/// reach the remote, carrying the warning the CLI surfaces in its JSON.
#[allow(clippy::too_many_arguments)]
pub fn run_with_body(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    parent: Option<&str>,
    body: Option<&str>,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<(PathBuf, PushOutcome)> {
    let type_def = config.type_by_name(doc_type).ok_or_else(|| {
        anyhow!(
            "unknown doc type: '{}'. valid types: {}",
            doc_type,
            config
                .documents
                .types
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    if type_def.singleton {
        let existing: Vec<_> = store.list(&Filter {
            doc_type: Some(DocType::new(doc_type)),
            ..Default::default()
        });
        if let Some(doc) = existing.first() {
            bail!("{} already exists at {}", doc_type, doc.path.display());
        }
    }

    // Ahead of the git early return below: a git child asked for a parent goes
    // through `create_with_parent`, whose same-repo guard applies to git types
    // too and whose write lands (and commits) inside the parent's clone.
    if let Some(parent_id) = parent {
        return create_with_parent(
            root, config, store, type_def, title, author, body, parent_id,
        )
        .map(|path| (path, PushOutcome::Synced));
    }

    if type_def.store == StoreBackend::Git {
        let mut registry = build_registry(root, config);
        let created =
            registry
                .for_type(type_def)?
                .create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GithubIssues {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no [github] config found",
                doc_type
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no github.repo configured",
                doc_type
            )
        })?;
        let mut store = GithubIssuesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
            issue_cache: IssueCache::new(root),
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GithubMilestones {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-milestones store but no [github] config found",
                doc_type
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-milestones store but no github.repo configured",
                doc_type
            )
        })?;
        let mut store = GithubMilestonesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GithubProjects {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-projects store but no [github] config found",
                doc_type
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-projects store but no github.repo configured",
                doc_type
            )
        })?;
        let mut store = crate::engine::store_dispatch::GithubProjectsStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GitRef {
        let mut store = GitRefStore {
            git: Box::new(GitCli),
            root: root.to_path_buf(),
            config: config.clone(),
            remote: config.git_ref.remote.clone(),
            reserved_number: None,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::ClickupTasks {
        // The registry leaves the ClickUp store's token unloaded to keep
        // registry construction free of keychain I/O; the shared write-store
        // helper loads it (from the global credential store: keychain-first,
        // file fallback, never a repo-local file) and binds a token-bearing
        // store.
        let mut store = crate::engine::store_dispatch::clickup_write_store(
            root,
            config,
            "creating",
            ClickupHttpClient::new,
            || LayeredCredentialStore::global().load_clickup_token(),
        )?;
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    let path = fs_ops::create_document(
        root,
        config,
        doc_type,
        &type_def.dir,
        &type_def.prefix,
        title,
        author,
        &type_def.numbering,
        type_def.subdirectory,
        None,
        on_progress,
    )?;

    if let Some(body_text) = body {
        fs_ops::replace_body(&path, body_text)?;
    }

    Ok((path, PushOutcome::Synced))
}

/// Author a child of `parent_id`, branching on the child type's store.
///
/// For github-issues children the child becomes a REAL GitHub issue bound as a
/// native sub-issue of the parent immediately at create time (via
/// [`GithubIssuesStore::create_child_subissue`]) -- no local-only `.md` is left
/// behind. For filesystem (and any other) store the child is written as a
/// sibling `.md` inside the parent's subdirectory, promoting a flat parent to
/// `TYPE-n-slug/index.md` on the first child; the loader tracks the new
/// parent/child edges directly.
///
/// Both branches enforce the same-repo guard: parent and child must resolve
/// to the same repo -- [`same_repo`] -- not merely share a [`StoreBackend`],
/// since two `git` types can declare different `remote`/`branch` pairs.
#[allow(clippy::too_many_arguments)]
fn create_with_parent(
    root: &Path,
    config: &Config,
    store: &Store,
    child_type_def: &TypeDef,
    title: &str,
    author: &str,
    body: Option<&str>,
    parent_id: &str,
) -> Result<PathBuf> {
    let parent_meta = store
        .resolve_shorthand(parent_id)
        .map_err(|_| anyhow!("could not resolve parent document: {}", parent_id))?;

    let parent_type_def = config
        .type_by_name(parent_meta.doc_type.as_str())
        .ok_or_else(|| {
            anyhow!(
                "parent {} has unknown type '{}'",
                parent_id,
                parent_meta.doc_type
            )
        })?;

    if !same_repo(child_type_def, parent_type_def) {
        let mut msg = format!(
            "sub-issue link rejected: parent {} (store {}) and child type {} (store {}) \
             are in different stores; lazyspec sub-issues are same-store only",
            parent_id, parent_type_def.store, child_type_def.name, child_type_def.store
        );
        if child_type_def.store == StoreBackend::Git && parent_type_def.store == StoreBackend::Git {
            msg = format!(
                "{msg} (parent remote {}, child remote {})",
                parent_type_def.remote.as_deref().unwrap_or("<none>"),
                child_type_def.remote.as_deref().unwrap_or("<none>"),
            );
        }
        bail!(msg);
    }

    if child_type_def.store == StoreBackend::GithubIssues {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no [github] config found",
                child_type_def.name
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no github.repo configured",
                child_type_def.name
            )
        })?;
        let mut gh_store = GithubIssuesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
            issue_cache: IssueCache::new(root),
        };
        let created = gh_store.create_child_subissue(
            child_type_def,
            parent_id,
            title,
            author,
            body.unwrap_or(""),
        )?;
        return Ok(root.join(&created.path));
    }

    let parent_path = root.join(&parent_meta.path);
    let is_index = parent_path
        .file_name()
        .and_then(|f| f.to_str())
        .map(|f| f == "index.md")
        .unwrap_or(false);

    let parent_subdir = if is_index {
        parent_path
            .parent()
            .ok_or_else(|| {
                anyhow!(
                    "parent index.md has no directory: {}",
                    parent_path.display()
                )
            })?
            .to_path_buf()
    } else {
        let parent_dir = parent_path
            .parent()
            .ok_or_else(|| anyhow!("parent doc has no directory: {}", parent_path.display()))?;
        let stem = parent_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("parent doc has no file stem: {}", parent_path.display()))?;
        let new_dir = parent_dir.join(stem);
        let new_index = new_dir.join("index.md");
        fs::create_dir_all(&new_dir)?;
        fs::rename(&parent_path, &new_index)?;
        new_dir
    };

    let child_path = fs_ops::create_child_in_dir(
        root,
        config,
        child_type_def,
        &parent_subdir,
        title,
        author,
        body,
    )?;

    // Keyed on the parent's path, not the child's: that is the clone the
    // rename and the new file both landed in, even when child and parent are
    // two `git` types sharing a remote (RFC-072 "The git store").
    commit_if_git_backed(
        root,
        config,
        &parent_meta.path,
        &GitCli,
        &format!("create child of {parent_id}"),
    )?;

    Ok(child_path)
}

/// Same store, same remote, same branch: the repo two types resolve to, not
/// just their [`StoreBackend`] discriminant. `remote`/`branch` are `None` on
/// every non-`git` type (`Config::parse` rejects them otherwise), so this
/// degrades to a plain store comparison for every backend but `git`.
fn same_repo(a: &TypeDef, b: &TypeDef) -> bool {
    a.store == b.store && a.remote == b.remote && a.branch == b.branch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::store::test_support::doc_md;
    use crate::engine::store::Store;
    use tempfile::TempDir;

    /// Docs on disk under a fresh `TempDir`, with an empty `.lazyspec/cache/<name>`
    /// pre-created for every `git`-typed name in `precloned` -- `Store::load`
    /// clones only when the cache dir is missing, and these tests exercise the
    /// same-repo guard, not a real clone (DICTUM-004: no network in a unit test).
    fn project(files: &[(&str, &str)], precloned: &[&str], config: &Config) -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        for (rel_path, contents) in files {
            let full = tmp.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        for name in precloned {
            std::fs::create_dir_all(tmp.path().join(".lazyspec/cache").join(name)).unwrap();
        }
        let store = Store::load(tmp.path(), config).unwrap();
        (tmp, store)
    }

    fn git_type(name: &str, remote: &str, branch: Option<&str>) -> TypeDef {
        TypeDef {
            remote: Some(remote.to_string()),
            branch: branch.map(str::to_string),
            ..TypeDef::test_fixture(name, StoreBackend::Git)
        }
    }

    // --- same_repo (AC9, AC10): the resolved repo, not just the discriminant ---

    #[test]
    fn same_repo_true_for_two_filesystem_types() {
        let a = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        let b = TypeDef::test_fixture("story", StoreBackend::Filesystem);

        assert!(same_repo(&a, &b));
    }

    #[test]
    fn same_repo_true_for_two_git_types_with_equal_remote_and_branch() {
        let a = git_type("rfc", "https://example.com/x.git", Some("next"));
        let b = git_type("spec", "https://example.com/x.git", Some("next"));

        assert!(same_repo(&a, &b));
    }

    #[test]
    fn same_repo_false_when_remote_differs() {
        let a = git_type("rfc", "https://example.com/a.git", Some("next"));
        let b = git_type("spec", "https://example.com/b.git", Some("next"));

        assert!(!same_repo(&a, &b));
    }

    #[test]
    fn same_repo_false_when_only_branch_differs() {
        let a = git_type("rfc", "https://example.com/x.git", Some("next"));
        let b = git_type("spec", "https://example.com/x.git", Some("main"));

        assert!(!same_repo(&a, &b));
    }

    // --- create_with_parent: the guard rejects before any mutation ---

    #[test]
    fn create_with_parent_filesystem_parent_git_child_rejected_unchanged_text() {
        let parent_type = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        let child_type = git_type("spec", "https://example.com/child.git", None);
        let mut config = Config::default();
        config.documents.types = vec![parent_type, child_type.clone()];
        let (tmp, store) = project(
            &[("docs/rfc/RFC-001-a.md", &doc_md("A", "rfc", "[]"))],
            &["spec"],
            &config,
        );

        let err = create_with_parent(
            tmp.path(),
            &config,
            &store,
            &child_type,
            "Child",
            "tester",
            None,
            "RFC-001",
        )
        .unwrap_err();

        assert!(err.to_string().contains("different stores"), "{err}");
    }

    #[test]
    fn create_with_parent_two_git_types_different_remotes_rejected_before_mutation() {
        let parent_type = git_type("a", "/a.git", None);
        let child_type = git_type("b", "/b.git", None);
        let mut config = Config::default();
        config.documents.types = vec![
            TypeDef {
                dir: "docs/a".to_string(),
                ..parent_type
            },
            TypeDef {
                dir: "docs/b".to_string(),
                ..child_type
            },
        ];
        let child_type_def = config.documents.types[1].clone();
        let (tmp, store) = project(
            &[(
                ".lazyspec/cache/a/docs/a/A-001-parent.md",
                &doc_md("Parent", "a", "[]"),
            )],
            &["b"],
            &config,
        );

        let err = create_with_parent(
            tmp.path(),
            &config,
            &store,
            &child_type_def,
            "Child",
            "tester",
            None,
            "A-001",
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("/a.git"), "{msg}");
        assert!(msg.contains("/b.git"), "{msg}");
        assert!(
            !tmp.path().join(".lazyspec/cache/b/docs/b").exists(),
            "no file written under the child's own cache dir"
        );
        assert_eq!(
            fs::read_dir(tmp.path().join(".lazyspec/cache/a/docs/a"))
                .unwrap()
                .count(),
            1,
            "no file written under the parent's cache dir"
        );
    }
}
