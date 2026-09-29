use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::fs::FileSystem;
use crate::engine::fs_ops;
use crate::engine::git_ref::GitRefOps;
use crate::engine::hooks::HookEnv;
use crate::engine::ops::resolve::resolve_shorthand_or_path;
use crate::engine::pre_transition::{self, updates_are_current, PlannedUpdate, ReportedFinding};
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, PushOutcome};
use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

/// `update <id> --part <name> --body|--body-file` (STORY-291 AC7): write a
/// part's whole body, creating the part if the document has none by that
/// name. Parts are a filesystem concept (a `.md` beside `index.md`), so this
/// refuses any backend that is not one -- filesystem, and a `git` store,
/// which scaffolds and reads a bundle the same way from inside its clone
/// (STORY-291 AC2/AC7) -- rather than silently doing nothing.
pub fn run_part(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_id: &str,
    part_name: &str,
    body: &str,
    git: &dyn GitRefOps,
) -> Result<PushOutcome> {
    let doc = resolve_shorthand_or_path(store, doc_id)?;
    let type_def = config
        .type_by_name(doc.doc_type.as_str())
        .ok_or_else(|| anyhow!("document {} has unknown type '{}'", doc_id, doc.doc_type))?;
    if !matches!(type_def.store, StoreBackend::Filesystem | StoreBackend::Git) {
        bail!(
            "'--part' is only supported for filesystem- or git-backed documents; type '{}' uses store '{}'",
            type_def.name,
            type_def.store
        );
    }

    fs_ops::write_part(root, store, doc_id, part_name, body)?;
    crate::engine::git_store::commit_if_git_backed_outcome(
        root,
        config,
        &doc.path,
        git,
        &format!("update part {} of {}", part_name, doc.id),
    )
}

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

/// What a move that ran hooks did: the outcome of saving it, the findings that
/// did not block it, and every file it wrote, so a surface holding the store
/// can reload them.
#[derive(Debug)]
pub struct TransitionOutcome {
    pub push: PushOutcome,
    pub findings: Vec<ReportedFinding>,
    pub updates: Vec<PlannedUpdate>,
    pub touched: Vec<PathBuf>,
}

/// Put back what `saved` replaced, newest first. A store that refuses a
/// restore leaves that one document as the hook made it; those are returned
/// as `"<id> (<reason>)"` so the caller can name them.
fn roll_back(
    saved: &[&PlannedUpdate],
    root: &Path,
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
) -> Vec<String> {
    saved
        .iter()
        .rev()
        .filter_map(|update| {
            write_update(update, &update.original, root, store, config, git)
                .err()
                .map(|e| format!("{} ({e})", update.id))
        })
        .collect()
}

/// `failure` with the outcome of rolling back: "nothing was saved" only when
/// every restore worked, otherwise the documents left changed.
fn rolled_back_error(
    failure: anyhow::Error,
    context: &str,
    unrestored: Vec<String>,
) -> anyhow::Error {
    if unrestored.is_empty() {
        return failure.context(format!("{context}; nothing was saved"));
    }
    failure.context(format!(
        "{context}; rolling back failed, so these documents are left changed by the hooks: {}",
        unrestored.join(", ")
    ))
}

fn write_update(
    update: &PlannedUpdate,
    body: &str,
    root: &Path,
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
) -> Result<()> {
    match &update.part {
        Some(part) => run_part(root, config, store, &update.id, part, body, git).map(|_| ()),
        None => write_doc(
            root,
            store,
            &update.id,
            &[("body", body)],
            Some(config),
            git,
        )
        .map(|_| ()),
    }
}

/// Save `updates`, then `status_update`, through the `update --body` / `--part` path.
/// A failure part-way puts back what was already written, so nothing is saved.
fn save_together(
    updates: &[PlannedUpdate],
    status_update: Option<(&str, &[(&str, &str)])>,
    root: &Path,
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    fs: &dyn FileSystem,
) -> Result<PushOutcome> {
    if !updates_are_current(updates, store, root, fs) {
        bail!("a document a hook updated changed while the hooks ran; nothing was saved");
    }
    let mut saved: Vec<&PlannedUpdate> = Vec::new();
    for update in updates {
        if let Err(e) = write_update(update, &update.body, root, store, config, git) {
            let unrestored = roll_back(&saved, root, store, config, git);
            let context = format!("saving the update to {}", update.id);
            return Err(rolled_back_error(e, &context, unrestored));
        }
        saved.push(update);
    }
    let Some((doc_path, status_update)) = status_update else {
        return Ok(PushOutcome::Synced);
    };
    write_doc(root, store, doc_path, status_update, Some(config), git).map_err(|e| {
        if saved.is_empty() {
            return e;
        }
        let unrestored = roll_back(&saved, root, store, config, git);
        rolled_back_error(e, "saving the status", unrestored)
    })
}

fn touched(doc_path: &Path, updates: &[PlannedUpdate]) -> Vec<PathBuf> {
    let mut paths = vec![doc_path.to_path_buf()];
    for update in updates {
        if !paths.contains(&update.path) {
            paths.push(update.path.clone());
        }
    }
    paths
}

/// `update` (STORY-296): when `updates` moves the status, run the `pre-transition`
/// hooks that match the move, and save what they ask to update together with the
/// status. An error finding surfaces as [`TransitionBlocked`]. Every status
/// change reaches the store through here, so a caller cannot skip the hooks:
/// only `env.disabled` (`--no-hooks`) does.
pub fn run_with_config(
    env: &HookEnv,
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
    config: &Config,
    git: &dyn GitRefOps,
) -> Result<TransitionOutcome> {
    let doc = resolve_shorthand_or_path(store, doc_path)?;
    let target = updates
        .iter()
        .find(|(k, _)| *k == "status")
        .map(|(_, target)| *target)
        .filter(|target| *target != doc.status.as_str());
    let Some(target) = target else {
        let push = write_doc_gated(root, store, doc_path, updates, config, git)?;
        return Ok(TransitionOutcome {
            push,
            findings: Vec::new(),
            updates: Vec::new(),
            touched: vec![doc.path.clone()],
        });
    };
    if let Some(type_def) = config.type_by_name(doc.doc_type.as_str()) {
        check_status_gate(root, type_def, doc.status.as_str(), target)?;
    }
    let cleared =
        pre_transition::check(env, root, store, config, doc, doc.status.as_str(), target)?;
    let push = save_together(
        &cleared.updates,
        Some((doc_path, updates)),
        root,
        store,
        config,
        git,
        &*env.fs,
    )?;
    Ok(TransitionOutcome {
        push,
        findings: cleared.findings,
        touched: touched(&doc.path, &cleared.updates),
        updates: cleared.updates,
    })
}

/// `hook run pre-transition <id>` (STORY-296 AC4): fire the hooks with `from`
/// and `to` both the current status. Saves their updates unless `dry_run`; never
/// touches the status.
pub fn run_hooks_by_hand(
    env: &HookEnv,
    root: &Path,
    store: &Store,
    doc_id: &str,
    dry_run: bool,
    config: &Config,
    git: &dyn GitRefOps,
) -> Result<TransitionOutcome> {
    let doc = resolve_shorthand_or_path(store, doc_id)?;
    let status = doc.status.as_str();
    let cleared = pre_transition::check(env, root, store, config, doc, status, status)?;
    let mut push = PushOutcome::Synced;
    let mut touched_paths = Vec::new();
    if !dry_run {
        push = save_together(&cleared.updates, None, root, store, config, git, &*env.fs)?;
        touched_paths = touched(&doc.path, &cleared.updates);
    }
    Ok(TransitionOutcome {
        push,
        findings: cleared.findings,
        updates: cleared.updates,
        touched: touched_paths,
    })
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

/// A type whose lifecycle is an authority board's columns is gated on the state
/// that board column resolves to, and rejects a value naming no column at all --
/// both offline, from the cached schema snapshot, before any store (and so any
/// client) is built. That is what makes the rejection reachable with no network.
fn check_status_gate(root: &Path, type_def: &TypeDef, current: &str, target: &str) -> Result<()> {
    let board_state =
        crate::engine::store_dispatch::resolve_authority_status_write(root, type_def, target)?
            .map(|write| write.state);
    gate_status_transition(type_def, current, board_state.as_deref().unwrap_or(target))
}

/// [`write_doc`] for a write nothing has gated yet: a `status` in `updates` is
/// checked against the type's lifecycle first.
fn write_doc_gated(
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
    config: &Config,
    git: &dyn GitRefOps,
) -> Result<PushOutcome> {
    if let Some((_, target)) = updates.iter().find(|(k, _)| *k == "status") {
        let doc = resolve_shorthand_or_path(store, doc_path)?;
        if let Some(type_def) = config.type_by_name(doc.doc_type.as_str()) {
            check_status_gate(root, type_def, doc.status.as_str(), target)?;
        }
    }
    write_doc(root, store, doc_path, updates, Some(config), git)
}

/// Writes without checking the status gate: [`run_with_config`] has already
/// checked it before the hooks ran, and the update bodies carry no status.
fn write_doc(
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

            fs_ops::update_document_with_type(root, store, doc_path, updates, Some(type_def))?;
            return crate::engine::git_store::commit_if_extends_backed(
                root,
                config,
                type_def,
                git,
                &format!("update {}", doc.id),
            );
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
            &HookEnv::disabled(),
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            &config,
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
            &HookEnv::disabled(),
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            &config,
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
            &HookEnv::disabled(),
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            &config,
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
            &HookEnv::disabled(),
            tmp.path(),
            &store,
            "RFC-001",
            &[("title", "Renamed"), ("assignee", "alice")],
            &config,
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

    // STORY-291 AC2/AC7 follow-up: `--part` is a filesystem concept, and a
    // `git` store's documents live on disk inside its clone the same way, so
    // `run_part` must write and commit there rather than reject it as an
    // unsupported backend.
    fn git_bundle_project() -> (TempDir, Config, TypeDef, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let remote = "https://example.com/change.git";
        let td = TypeDef {
            dir: "docs/change".to_string(),
            remote: Some(remote.to_string()),
            branch: Some("next".to_string()),
            subdirectory: true,
            ..TypeDef::test_fixture("change", StoreBackend::Git)
        };
        let clone_root = crate::engine::git_store::clone_root(tmp.path(), remote, Some("next"));
        let doc_dir = clone_root.join("docs/change/CHANGE-001-alpha");
        std::fs::create_dir_all(&doc_dir).unwrap();
        std::fs::write(
            doc_dir.join("index.md"),
            "---\ntitle: \"Alpha\"\ntype: change\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n\nbody\n",
        )
        .unwrap();
        std::fs::write(doc_dir.join("design.md"), "old design\n").unwrap();

        let mut config = Config::default();
        config.documents.types = vec![td.clone()];
        (tmp, config, td, doc_dir)
    }

    #[test]
    fn run_part_writes_and_commits_a_git_backed_bundle() {
        let (tmp, config, _td, doc_dir) = git_bundle_project();
        let store = Store::load(tmp.path(), &config).unwrap();
        let git = MockGitRefClient::new().with_has_uncommitted_changes_result(Ok(true));

        let outcome = run_part(
            tmp.path(),
            &config,
            &store,
            "CHANGE-001",
            "design",
            "new design",
            &git,
        )
        .unwrap();

        assert!(
            matches!(outcome, PushOutcome::LocalOnly { .. }),
            "got: {outcome:?}"
        );
        assert_eq!(
            std::fs::read_to_string(doc_dir.join("design.md")).unwrap(),
            "new design\n"
        );
    }

    // A backend that is neither filesystem nor git (a part has nowhere to
    // live for one) is still rejected by name.
    #[test]
    fn run_part_rejects_a_backend_with_no_filesystem_shape() {
        let tmp = TempDir::new().unwrap();
        let doc = "---\ntitle: \"T\"\ntype: task\nstatus: open\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n\nbody\n";
        std::fs::create_dir_all(tmp.path().join(".lazyspec/cache/task")).unwrap();
        std::fs::write(tmp.path().join(".lazyspec/cache/task/TASK-001-t.md"), doc).unwrap();
        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture("task", StoreBackend::ClickupTasks)];
        let store = Store::load(tmp.path(), &config).unwrap();
        let git = MockGitRefClient::new();

        let err = run_part(
            tmp.path(),
            &config,
            &store,
            "TASK-001",
            "design",
            "new design",
            &git,
        )
        .unwrap_err();

        assert!(err.to_string().contains("clickup"), "{err}");
    }

    fn planned_body(store: &Store, root: &Path, body: &str, hash: Option<&str>) -> PlannedUpdate {
        let doc = store.resolve_shorthand("RFC-001").unwrap();
        let snap =
            crate::engine::hooks::hook_document(doc, root, &crate::engine::fs::RealFileSystem);
        PlannedUpdate {
            id: doc.id.clone(),
            path: doc.path.clone(),
            part: None,
            body: body.to_string(),
            hash: hash
                .map(str::to_string)
                .unwrap_or_else(|| snap["content_hash"].as_str().unwrap().to_string()),
            original: snap["body"].as_str().unwrap().to_string(),
        }
    }

    /// STORY-296 AC3: a document that moved since the hook read it saves nothing,
    /// not even the status.
    #[test]
    fn a_stale_update_saves_neither_the_body_nor_the_status() {
        let config = Config::default();
        let (tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new();
        let update = planned_body(&store, tmp.path(), "hooked", Some("stale"));

        let err = save_together(
            &[update],
            Some(("RFC-001", &[("status", "review")])),
            tmp.path(),
            &store,
            &config,
            &git,
            &crate::engine::fs::RealFileSystem,
        )
        .unwrap_err();

        assert!(err.to_string().contains("nothing was saved"), "{err}");
        let content = doc_on_disk(tmp.path());
        assert!(content.contains("status: draft") && !content.contains("hooked"));
    }

    /// STORY-296 AC3: a status that cannot be saved takes the hook's updates
    /// back out.
    #[test]
    fn a_failed_status_write_puts_the_updated_body_back() {
        let config = Config::default();
        let (tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new();
        let original = doc_on_disk(tmp.path());
        let update = planned_body(&store, tmp.path(), "hooked", None);

        let result = save_together(
            &[update],
            Some(("RFC-999", &[("status", "review")])),
            tmp.path(),
            &store,
            &config,
            &git,
            &crate::engine::fs::RealFileSystem,
        );

        let err = result.unwrap_err();
        assert!(err.to_string().contains("nothing was saved"), "{err}");
        let content = doc_on_disk(tmp.path());
        assert!(!content.contains("hooked"), "{content}");
        assert_eq!(content, original);
    }

    /// STORY-296 AC3: the second of two updates failing puts the first back.
    #[test]
    fn a_failed_second_update_restores_the_first() {
        let config = Config::default();
        let (tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new();
        let original = doc_on_disk(tmp.path());
        let first = planned_body(&store, tmp.path(), "hooked", None);
        let second = PlannedUpdate {
            id: "RFC-404".to_string(),
            path: PathBuf::from("docs/rfcs/RFC-404-missing.md"),
            part: None,
            body: "hooked".to_string(),
            hash: String::new(),
            original: String::new(),
        };

        let err = save_together(
            &[first, second],
            None,
            tmp.path(),
            &store,
            &config,
            &git,
            &crate::engine::fs::RealFileSystem,
        );

        let message = format!("{:#}", err.unwrap_err());
        assert!(message.contains("nothing was saved"), "{message}");
        assert_eq!(doc_on_disk(tmp.path()), original);
    }

    /// STORY-296 AC3: a restore the store refuses is named, and the error does
    /// not claim nothing was saved.
    #[test]
    fn a_failed_restore_names_the_document_left_changed() {
        let config = Config::default();
        let (tmp, store) = fs_store(None, &config);
        let git = MockGitRefClient::new();
        let gone = PlannedUpdate {
            id: "RFC-404".to_string(),
            path: PathBuf::from("docs/rfcs/RFC-404-missing.md"),
            part: None,
            body: "hooked".to_string(),
            hash: String::new(),
            original: "old".to_string(),
        };

        let unrestored = roll_back(&[&gone], tmp.path(), &store, &config, &git);
        assert_eq!(unrestored.len(), 1);
        assert!(unrestored[0].starts_with("RFC-404"), "{unrestored:?}");

        let err = rolled_back_error(anyhow!("boom"), "saving the update to RFC-001", unrestored);
        let message = format!("{err:#}");
        assert!(message.contains("RFC-404"), "{message}");
        assert!(!message.contains("nothing was saved"), "{message}");
    }
}
