use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::fs_ops;
use crate::engine::git_ref::GitRefOps;
use crate::engine::ops::resolve::resolve_shorthand_or_path;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, PushOutcome};
use anyhow::{bail, Result};
use std::path::Path;

/// Gate a `--status` change against the type's local lifecycle before it reaches
/// the backend.
///
/// Filesystem- and GitHub-backed types own their transition DAG locally: a
/// target must be an out-edge of the current status (or a no-op to the same
/// status). ClickUp-backed types carry no local edges -- their lifecycle states
/// mirror the bound List's status set and ClickUp enforces its own transition
/// rules (RFC-056 §Status handling) -- so lazyspec applies no gate and lets the
/// raw status string through; ClickUp validates and rejects an illegal target.
fn gate_status_transition(type_def: &TypeDef, current: &str, target: &str) -> Result<()> {
    if type_def.store == StoreBackend::ClickupTasks {
        return Ok(());
    }
    let lifecycle = type_def.effective_lifecycle();
    if current != target && !lifecycle.has_edge(current, target) {
        let allowed = lifecycle.targets_from(current);
        let allowed = if allowed.is_empty() {
            "(none)".to_string()
        } else {
            allowed.join(", ")
        };
        bail!(
            "invalid transition for type \"{}\": no edge from \"{}\" to \"{}\" (allowed targets: {})",
            type_def.name,
            current,
            target,
            allowed
        );
    }
    Ok(())
}

pub fn run(
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
    git: &dyn GitRefOps,
) -> Result<PushOutcome> {
    run_with_config(root, store, doc_path, updates, None, git)
}

/// The commit to stamp `reviewed` with when `updates` moves the status, or
/// `None` when it does not, when the type's backend cannot hold an anchor, or
/// when `HEAD` cannot be read (RFC-069, STORY-274 AC6): a repository with no
/// commits still transitions, it just keeps whatever anchor it had.
///
/// Filesystem writes the anchor to the document's own frontmatter;
/// github-issues carries it in the issue body, which `fetch` rebuilds the cache
/// file from. The clickup, git-ref and milestone cache builders hardcode
/// `reviewed: None`, so an anchor stamped for them would be dropped by the next
/// read -- they are excluded permanently, not pending a slice.
///
/// HEAD comes from `[governs] root`, as `pin` reads it: that is the repository
/// `diff_stat` runs the anchor against, and under a docs-repo split a docs sha
/// means nothing there.
fn review_stamp(
    store: &Store,
    type_def: &TypeDef,
    updates: &[(&str, &str)],
    git: &dyn GitRefOps,
) -> Option<String> {
    if !matches!(
        type_def.store,
        StoreBackend::Filesystem | StoreBackend::GithubIssues
    ) {
        return None;
    }
    if !updates.iter().any(|(key, _)| *key == "status") {
        return None;
    }
    git.head(store.governs_root()).ok()
}

pub fn run_with_config(
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
    config: Option<&Config>,
    git: &dyn GitRefOps,
) -> Result<PushOutcome> {
    if let Some(config) = config {
        let doc = resolve_shorthand_or_path(store, doc_path)?;
        let type_name = doc.doc_type.as_str();
        if let Some(type_def) = config.type_by_name(type_name) {
            if let Some((_, target)) = updates.iter().find(|(k, _)| *k == "status") {
                // A type whose lifecycle is an authority board's columns is gated
                // on the state that board column resolves to, and rejects a value
                // naming no column at all -- both offline, from the cached schema
                // snapshot, before any store (and so any client) is built. That is
                // what makes the rejection reachable with no network.
                let board_state = crate::engine::store_dispatch::resolve_authority_status_write(
                    root, type_def, target,
                )?
                .map(|write| write.state);
                gate_status_transition(
                    type_def,
                    doc.status.as_str(),
                    board_state.as_deref().unwrap_or(target),
                )?;
            }
            // A local transition is a human declaring the document true against
            // the code in front of them, so it resets the staleness clock in the
            // same write as the status -- never a second pass. A status arriving
            // through `fetch` stamps nothing: `sync_all` never calls this
            // function and holds no `GitRefOps` to read a HEAD with.
            let stamp = review_stamp(store, type_def, updates, git);
            let mut updates = updates.to_vec();
            if let Some(sha) = &stamp {
                updates.push(("reviewed", sha.as_str()));
            }
            let updates = updates.as_slice();

            // Non-filesystem backends dispatch through the store registry; a new
            // backend routes here by being registered in `build_registry`, not by
            // adding another branch. Filesystem keeps its dedicated `fs_ops` path
            // (it edits the document in place by its original path, not by id).
            if type_def.store != StoreBackend::Filesystem {
                // ClickUp authenticates per write: the registry leaves its token
                // unloaded (to keep registry construction free of keychain I/O),
                // so the write path loads the global credential here -- mirroring
                // the create path -- and dispatches against a token-bearing store.
                // A registry-built (token: None) ClickUp store would fail the
                // write on missing auth.
                if type_def.store == StoreBackend::ClickupTasks {
                    let mut store = crate::engine::store_dispatch::clickup_write_store(
                        root,
                        config,
                        "updating",
                        ClickupHttpClient::new,
                        || LayeredCredentialStore::global().load_clickup_token(),
                    )?;
                    return store.update(type_def, &doc.id, updates);
                }
                let mut registry = crate::engine::store_dispatch::build_registry(root, config);
                return registry
                    .for_type(type_def)?
                    .update(type_def, &doc.id, updates);
            }

            return fs_ops::update_document_with_type(
                root,
                store,
                doc_path,
                updates,
                Some(type_def),
            )
            .map(|_| PushOutcome::Synced);
        }
    }

    fs_ops::update_document(root, store, doc_path, updates).map(|_| PushOutcome::Synced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::DocMeta;
    use crate::engine::git_ref::test_support::{MockGitRefClient, FAKE_HEAD};
    use crate::engine::store::test_support::store_from_with_config;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const RFC: &str = "docs/rfcs/RFC-001-engine.md";
    const OLD_ANCHOR: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    /// One filesystem-backed rfc on disk, loaded through `Store::load` so the
    /// update path resolves and rewrites it exactly as production does.
    fn fs_store(reviewed: Option<&str>, config: &Config) -> (TempDir, Store) {
        let reviewed_line = reviewed.map_or(String::new(), |sha| format!("reviewed: {sha}\n"));
        let doc = format!(
            "---\ntitle: \"Engine\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\n{reviewed_line}related: []\n---\n\nbody\n"
        );
        store_from_with_config(&[(RFC, &doc)], config)
    }

    fn doc_on_disk(root: &Path) -> String {
        std::fs::read_to_string(root.join(RFC)).unwrap()
    }

    fn reloaded(root: &Path, config: &Config) -> DocMeta {
        Store::load(root, config)
            .unwrap()
            .resolve_shorthand("RFC-001")
            .unwrap()
            .clone()
    }

    fn head_calls(git: &MockGitRefClient) -> Vec<String> {
        git.call_log()
            .borrow()
            .iter()
            .filter(|call| call.starts_with("head:"))
            .cloned()
            .collect()
    }

    /// STORY-274 AC1. HEAD is read from `[governs] root`, not the docs root:
    /// that is the repository `diff_stat` resolves the anchor in, and under a
    /// docs-repo split the two are different repositories.
    #[test]
    fn a_status_move_stamps_reviewed_with_head_of_the_governs_root() {
        let mut config = Config::default();
        config.governs.root = PathBuf::from("code");
        let (tmp, store) = fs_store(None, &config);
        std::fs::create_dir_all(tmp.path().join("code")).unwrap();
        let git = MockGitRefClient::new();

        run_with_config(
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            Some(&config),
            &git,
        )
        .unwrap();

        let doc = reloaded(tmp.path(), &config);
        assert_eq!(doc.status.to_string(), "review");
        assert_eq!(doc.reviewed.as_deref(), Some(FAKE_HEAD));
        assert_eq!(
            head_calls(&git),
            [format!("head:{}", tmp.path().join("code").display())],
            "HEAD is read once, from [governs] root"
        );
    }

    /// STORY-274 AC6. A repository with no commits cannot answer `head`; the
    /// transition is still the point of the command, so it lands and the
    /// document simply keeps no anchor.
    #[test]
    fn an_unreadable_head_does_not_fail_the_transition_and_stamps_nothing() {
        let config = Config::default();
        let (tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new().with_head_result(Err(anyhow::anyhow!("no HEAD")));

        run_with_config(
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            Some(&config),
            &git,
        )
        .unwrap();

        let doc = reloaded(tmp.path(), &config);
        assert_eq!(doc.status.to_string(), "review");
        assert_eq!(doc.reviewed, None);
        assert!(
            !doc_on_disk(tmp.path()).contains("reviewed:"),
            "no anchor line is written at all"
        );
    }

    /// AC6's other half: an unreadable HEAD leaves an anchor the document
    /// already had alone rather than clearing it.
    #[test]
    fn an_unreadable_head_leaves_an_existing_reviewed_untouched() {
        let config = Config::default();
        let (tmp, store) = fs_store(Some(OLD_ANCHOR), &config);
        let git = MockGitRefClient::new().with_head_result(Err(anyhow::anyhow!("no HEAD")));

        run_with_config(
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            Some(&config),
            &git,
        )
        .unwrap();

        let content = doc_on_disk(tmp.path());
        assert_eq!(
            reloaded(tmp.path(), &config).reviewed.as_deref(),
            Some(OLD_ANCHOR)
        );
        assert_eq!(content.matches("reviewed:").count(), 1, "got: {content}");
    }

    /// Stamping is what a transition means, not what an edit means: retitling a
    /// document is not a claim that anyone re-read it against the code.
    #[test]
    fn an_update_without_a_status_asks_git_nothing_and_stamps_nothing() {
        let config = Config::default();
        let (tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new();

        run_with_config(
            tmp.path(),
            &store,
            "RFC-001",
            &[("title", "Renamed"), ("assignee", "alice")],
            Some(&config),
            &git,
        )
        .unwrap();

        assert_eq!(reloaded(tmp.path(), &config).reviewed, None);
        assert!(
            git.call_log().borrow().is_empty(),
            "git is not consulted at all: {:?}",
            git.call_log().borrow()
        );
    }

    /// STORY-274 AC5 and its limit. A github-issues type is stamped like a
    /// filesystem one -- the anchor rides the issue body, which `fetch` rebuilds
    /// the cache file from. The other three backends build their cache with
    /// `reviewed: None` hardcoded, so an anchor stamped for them would be
    /// dropped by the next read; git is not even asked.
    #[test]
    fn a_status_move_is_stamped_for_the_backends_whose_documents_can_hold_an_anchor() {
        let config = Config::default();
        let (_tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new();
        let status = [("status", "review")];

        for backend in [StoreBackend::Filesystem, StoreBackend::GithubIssues] {
            let td = TypeDef::test_fixture("rfc", backend);
            assert_eq!(
                review_stamp(&store, &td, &status, &git).as_deref(),
                Some(FAKE_HEAD),
                "{} documents hold a reviewed anchor",
                td.store
            );
        }
        for backend in [
            StoreBackend::GithubMilestones,
            StoreBackend::GithubProjects,
            StoreBackend::GitRef,
            StoreBackend::ClickupTasks,
        ] {
            let td = TypeDef::test_fixture("rfc", backend);
            assert_eq!(
                review_stamp(&store, &td, &status, &git),
                None,
                "{} caches cannot hold one",
                td.store
            );
        }
        assert_eq!(
            head_calls(&git).len(),
            2,
            "HEAD is read once per stampable backend and never for the rest: {:?}",
            git.call_log().borrow()
        );
    }

    // A filesystem/GitHub type still gates a status move on its lifecycle edges.
    #[test]
    fn gate_rejects_off_edge_move_for_edge_gated_type() {
        let td = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        // The default test fixture lifecycle carries no draft->accepted edge.
        let err = gate_status_transition(&td, "draft", "accepted").unwrap_err();
        assert!(err.to_string().contains("invalid transition"), "got: {err}");
    }

    // Why the authority resolution has to run BEFORE the gate: the gate compares
    // the target verbatim against the type's states, which a board lifecycle holds
    // lowercased. `--status "In Progress"` therefore only survives the gate as the
    // state the board column resolved to.
    #[test]
    fn gate_rejects_board_display_casing_but_accepts_the_resolved_state() {
        let mut td = TypeDef::test_fixture("ticket", StoreBackend::GithubIssues);
        td.status_authority = Some("PROJECT-7".to_string());
        td.lifecycle = crate::engine::config::Lifecycle {
            states: vec!["ready to start".into(), "in progress".into()],
            edges: vec![],
        };

        assert!(gate_status_transition(&td, "ready to start", "In Progress").is_err());
        gate_status_transition(&td, "ready to start", "in progress").unwrap();
    }

    // A ClickUp-backed type bypasses the local gate entirely: any status target
    // passes, because ClickUp (not lazyspec) owns the transition rules and the
    // derived lifecycle carries no edges (RFC-056 §Status handling).
    #[test]
    fn gate_bypasses_clickup_tasks_status_transition() {
        let td = TypeDef::test_fixture("task", StoreBackend::ClickupTasks);
        // A target that would be off-edge for any local DAG: still allowed.
        gate_status_transition(&td, "open", "in progress").unwrap();
        gate_status_transition(&td, "in progress", "done").unwrap();
    }
}
