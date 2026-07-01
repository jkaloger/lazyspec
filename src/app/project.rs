//! Project selection for the native shell (RFC-054 §"Project selection and app
//! config", STORY-186): validate that a chosen folder is a lazyspec project and
//! drive the launch-time folder picker until a valid project is chosen or the
//! user cancels.
//!
//! Layering (RFC-054 principle 3): this lives in `app`, consuming
//! [`crate::engine`]/[`crate::web`] load paths as-is. It adds no rendering.
//!
//! The interesting logic is deliberately Tauri-free so it is unit-testable
//! under `cargo test --features web` without spinning up a webview, mirroring
//! how [`super::protocol`] gates its adapter tests. Two seams are pure:
//! [`validate_project_root`] (the `.lazyspec/` predicate + plain-language
//! rejection) and [`run_picker_loop`] (pick -> validate -> re-prompt/exit),
//! which takes the native dialog calls as injected closures. The Tauri dialog
//! wiring that supplies those closures lives behind `#[cfg(feature = "app")]`
//! in [`super`].

use std::path::{Path, PathBuf};

/// The marker directory that identifies a folder as a lazyspec project root.
const PROJECT_MARKER: &str = ".lazyspec";

/// Build the shared [`AppState`](crate::web::server::AppState) for a project
/// root, mirroring how `lazyspec serve` constructs it in `main.rs` (STORY-185
/// AC8, `src/main.rs` `Serve` arm): load the store into an `Arc<Store>`, resolve
/// GitHub deep-link coords (deep-links disabled when unresolvable), load the
/// issue map, and derive the header repo/branch chips. No socket is involved.
///
/// This is the single app-side seam for turning a chosen folder into router
/// state. `run` calls it at launch; the in-session project switch (ITERATION-254)
/// re-calls it to rebuild the state behind the router, so it is factored out
/// here rather than inlined in [`super::run`] (RFC-054 principle 6: a second
/// consumer justifies the extraction). Gated on `web` — not `app` — so it
/// compiles and is unit-testable under `cargo test --features web` without
/// Tauri.
#[cfg(feature = "web")]
pub fn build_state(root: &Path) -> anyhow::Result<crate::web::server::AppState> {
    use crate::engine::config::Config;
    use crate::engine::issue_map::IssueMap;
    use crate::engine::store::Store;
    use std::sync::Arc;

    let fs = crate::engine::fs::RealFileSystem;
    let config = Config::load(root, &fs)?;
    let store = Store::load(root, &config)?;
    let coords = crate::engine::github_url::resolve_repo_coords(&config, root);
    let issue_map = Arc::new(IssueMap::load(root).unwrap_or_default());
    let repo_name = store
        .root()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let branch = crate::engine::git_status::query_git_branch(store.root());
    Ok(crate::web::server::AppState {
        store: crate::web::server::SharedStore::new(store),
        config: Arc::new(config),
        coords,
        issue_map,
        repo_name,
        branch,
    })
}

/// Outcome of driving the launch-time picker loop: either the user settled on a
/// valid project root, or they cancelled out of the picker. Callers match on
/// this to decide between opening a window and exiting cleanly, so it is a named
/// type rather than an `Option` (which reads ambiguously at the call site).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    /// The user chose a folder that validated as a lazyspec project.
    Selected(PathBuf),
    /// The user dismissed the picker without choosing a valid project.
    Cancelled,
}

/// Validate that `path` is a lazyspec project root: it must be a directory that
/// contains a `.lazyspec/` directory. On failure the `Err` carries a
/// plain-language message suitable for a native dialog (no stack trace, no
/// path-jargon-only text) — STORY-186 "plain-language rejection".
///
/// Returns `anyhow::Result<()>` per the codebase error-handling convention; the
/// error's `Display` is the user-facing message.
pub fn validate_project_root(path: &Path) -> anyhow::Result<()> {
    let name = folder_label(path);

    if !path.is_dir() {
        anyhow::bail!(
            "“{name}” isn’t a folder that can be opened. Please choose a project folder."
        );
    }

    let marker = path.join(PROJECT_MARKER);
    if !marker.is_dir() {
        anyhow::bail!(
            "“{name}” isn’t a lazyspec project — it has no {PROJECT_MARKER} folder inside it. Please choose a folder that contains a lazyspec project."
        );
    }

    Ok(())
}

/// A human-friendly label for a folder for use in messages: the final path
/// component, falling back to the full path when there is no file name (e.g. a
/// root path).
fn folder_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Drive the launch-time picker loop: repeatedly ask `pick` for a folder and
/// [`validate_project_root`] it. On an invalid choice, hand the plain-language
/// message to `notify_invalid` (a native error dialog) and re-open the picker;
/// on a valid choice, return [`PickOutcome::Selected`]; when `pick` yields
/// `None` (the user dismissed the picker), return [`PickOutcome::Cancelled`].
/// The loop never returns an invalid path, so callers cannot proceed to a view
/// on a bad selection (STORY-186 "never open a broken view").
///
/// The native dialog is injected as closures so the loop is exercised in tests
/// without Tauri (see module docs).
pub fn run_picker_loop(
    mut pick: impl FnMut() -> Option<PathBuf>,
    mut notify_invalid: impl FnMut(&str),
) -> PickOutcome {
    loop {
        match pick() {
            None => return PickOutcome::Cancelled,
            Some(path) => match validate_project_root(&path) {
                Ok(()) => return PickOutcome::Selected(path),
                Err(message) => notify_invalid(&message.to_string()),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Recents (STORY-186 "Recents persist across restarts in the platform config
// dir"): an ordered, deduped, most-recently-used-first list of project roots.
//
// The list logic is deliberately pure — ordering ([`prepend_recent`]) and the
// on-disk representation ([`parse_recents`]/[`serialize_recents`]) take and
// return plain values so they are unit-tested under `cargo test --features web`
// without touching the filesystem or Tauri. The thin I/O wrappers that resolve
// the platform config dir and read/write the file are gated on `app` (they pull
// `dirs`) and layer over these pure seams.
// ---------------------------------------------------------------------------

/// The file name of the recents list inside the app config directory. Only the
/// `app`-gated I/O wrappers reference it; the pure list logic is file-agnostic.
#[cfg(feature = "app")]
const RECENTS_FILE: &str = "recents.json";

/// The maximum number of remembered projects. Older entries beyond this are
/// dropped so the File > recents submenu stays bounded.
const MAX_RECENTS: usize = 20;

/// Return a new recents list with `path` moved to the front (most-recent-first),
/// removing any prior occurrence so the list stays deduped, and truncated to
/// [`MAX_RECENTS`]. Pure: no I/O, so the MRU/dedup rule is unit-tested directly.
///
/// Paths are compared verbatim (as stored). Callers pass an already-validated,
/// picker-supplied path; normalization is not attempted here because the picker
/// yields absolute paths and inventing a canonicalization rule would be an
/// untested guess.
pub fn prepend_recent(existing: &[PathBuf], path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(existing.len() + 1);
    out.push(path.to_path_buf());
    for p in existing {
        if p.as_path() != path {
            out.push(p.clone());
        }
    }
    out.truncate(MAX_RECENTS);
    out
}

/// Parse the recents file contents into an ordered list of paths. Tolerant by
/// design (STORY-186 "tolerate a missing/corrupt file as empty recents"): any
/// parse failure yields an empty list rather than an error, so a corrupt file
/// never blocks launch. Pure over the raw string.
pub fn parse_recents(contents: &str) -> Vec<PathBuf> {
    serde_json::from_str::<Vec<PathBuf>>(contents).unwrap_or_default()
}

/// Serialize a recents list to the on-disk string form. Pure counterpart to
/// [`parse_recents`]; the two round-trip.
pub fn serialize_recents(recents: &[PathBuf]) -> String {
    serde_json::to_string_pretty(recents).unwrap_or_else(|_| "[]".to_string())
}

/// Resolve the app config directory (`~/Library/Application Support/lazyspec/`
/// on macOS) via the `dirs` crate — never a hardcoded home path (STORY-186).
/// Returns `None` when the platform config dir cannot be resolved, in which case
/// callers treat recents as empty and non-persistent rather than failing.
#[cfg(feature = "app")]
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("lazyspec"))
}

/// The full path to the recents file under [`config_dir`].
#[cfg(feature = "app")]
pub fn recents_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(RECENTS_FILE))
}

/// Load the recents list from disk, tolerating a missing or corrupt file as an
/// empty list (STORY-186). Never errors: a bad recents file must not block
/// launch.
#[cfg(feature = "app")]
pub fn load_recents() -> Vec<PathBuf> {
    let Some(path) = recents_path() else {
        return Vec::new();
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => parse_recents(&contents),
        Err(_) => Vec::new(),
    }
}

/// Record `path` as the most-recent project: load the current list, move `path`
/// to the front (deduped, bounded), and write it back. Invoked on every
/// successful open — launch-open and switches (STORY-186 "added to recents").
/// Creates the config dir if needed. Write failures are surfaced to the caller
/// but are non-fatal to opening (the caller logs and proceeds).
#[cfg(feature = "app")]
pub fn record_recent(path: &Path) -> anyhow::Result<()> {
    let Some(file) = recents_path() else {
        anyhow::bail!("could not resolve the platform config directory for recents");
    };
    let updated = prepend_recent(&load_recents(), path);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, serialize_recents(&updated))?;
    Ok(())
}

/// Pick the project to reopen on launch (STORY-186 "Remembered project reopens"
/// / "…is now missing/moved"): the most-recent recents entry that still passes
/// [`validate_project_root`]. Entries that no longer exist or are no longer
/// lazyspec projects are skipped, so a stale head never opens a broken view; the
/// caller falls through to the picker when this returns `None`.
///
/// Pure over the supplied list (validation only touches the filesystem via
/// [`validate_project_root`]), so it is unit-testable with real temp dirs and no
/// Tauri.
pub fn most_recent_valid(recents: &[PathBuf]) -> Option<PathBuf> {
    recents
        .iter()
        .find(|p| validate_project_root(p).is_ok())
        .cloned()
}

#[cfg(all(test, feature = "web"))]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn build_state_loads_a_real_project_and_derives_the_repo_name_from_the_folder() {
        let parent = temp_dir();
        let root = parent.path().join("my-project");
        std::fs::create_dir(&root).unwrap();
        crate::cli::init::run(&root).expect("scaffold a lazyspec project");

        let state = build_state(&root).expect("build_state on a valid project");

        assert_eq!(
            state.repo_name, "my-project",
            "repo_name should be derived from the chosen folder name, as the serve arm does"
        );
        assert_eq!(
            state.store.snapshot().root().file_name(),
            Some(std::ffi::OsStr::new("my-project")),
            "the store should be rooted at the chosen folder"
        );
    }

    #[test]
    fn validate_accepts_a_folder_with_a_lazyspec_marker() {
        let dir = temp_dir();
        std::fs::create_dir(dir.path().join(".lazyspec")).unwrap();

        assert!(validate_project_root(dir.path()).is_ok());
    }

    #[test]
    fn validate_rejects_a_folder_without_a_lazyspec_marker() {
        let dir = temp_dir();

        let err = validate_project_root(dir.path()).unwrap_err().to_string();
        assert!(
            err.contains(".lazyspec"),
            "message should name the missing marker: {err}"
        );
        assert!(
            err.contains("lazyspec project"),
            "message should be plain-language, not jargon: {err}"
        );
    }

    #[test]
    fn validate_rejects_a_lazyspec_marker_that_is_a_file_not_a_dir() {
        let dir = temp_dir();
        std::fs::write(dir.path().join(".lazyspec"), b"not a dir").unwrap();

        assert!(validate_project_root(dir.path()).is_err());
    }

    #[test]
    fn validate_rejects_a_path_that_is_not_a_directory() {
        let dir = temp_dir();
        let file = dir.path().join("a-file.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(validate_project_root(&file).is_err());
    }

    #[test]
    fn picker_loop_selects_a_valid_folder_on_the_first_pick() {
        let dir = temp_dir();
        std::fs::create_dir(dir.path().join(".lazyspec")).unwrap();
        let chosen = dir.path().to_path_buf();

        let notifications = RefCell::new(Vec::<String>::new());
        let outcome = run_picker_loop(
            || Some(chosen.clone()),
            |m| notifications.borrow_mut().push(m.to_string()),
        );

        assert_eq!(outcome, PickOutcome::Selected(dir.path().to_path_buf()));
        assert!(
            notifications.borrow().is_empty(),
            "a first-try valid pick should not surface any rejection"
        );
    }

    #[test]
    fn picker_loop_reprompts_after_an_invalid_folder_then_accepts_a_valid_one() {
        let invalid = temp_dir();
        let valid = temp_dir();
        std::fs::create_dir(valid.path().join(".lazyspec")).unwrap();

        let picks = RefCell::new(vec![
            valid.path().to_path_buf(),
            invalid.path().to_path_buf(),
        ]);
        let notifications = RefCell::new(Vec::<String>::new());

        let outcome = run_picker_loop(
            || picks.borrow_mut().pop(),
            |m| notifications.borrow_mut().push(m.to_string()),
        );

        assert_eq!(outcome, PickOutcome::Selected(valid.path().to_path_buf()));
        assert_eq!(
            notifications.borrow().len(),
            1,
            "the invalid pick should have produced exactly one plain-language rejection"
        );
        assert!(notifications.borrow()[0].contains("lazyspec project"));
    }

    #[test]
    fn picker_loop_cancels_cleanly_when_the_picker_is_dismissed() {
        let notifications = RefCell::new(Vec::<String>::new());

        let outcome = run_picker_loop(|| None, |m| notifications.borrow_mut().push(m.to_string()));

        assert_eq!(outcome, PickOutcome::Cancelled);
        assert!(notifications.borrow().is_empty());
    }

    #[test]
    fn picker_loop_never_returns_an_invalid_path_before_cancelling() {
        let invalid = temp_dir();

        let picks = RefCell::new(vec![None, Some(invalid.path().to_path_buf())]);
        let notifications = RefCell::new(Vec::<String>::new());

        let outcome = run_picker_loop(
            || picks.borrow_mut().pop().flatten(),
            |m| notifications.borrow_mut().push(m.to_string()),
        );

        assert_eq!(
            outcome,
            PickOutcome::Cancelled,
            "an invalid pick followed by a dismiss must cancel, never select the invalid path"
        );
        assert_eq!(notifications.borrow().len(), 1);
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn prepend_puts_a_new_path_at_the_front() {
        let existing = vec![p("/a"), p("/b")];

        let out = prepend_recent(&existing, &p("/c"));

        assert_eq!(out, vec![p("/c"), p("/a"), p("/b")]);
    }

    #[test]
    fn prepend_moves_an_existing_path_to_the_front_without_duplicating() {
        let existing = vec![p("/a"), p("/b"), p("/c")];

        let out = prepend_recent(&existing, &p("/b"));

        assert_eq!(
            out,
            vec![p("/b"), p("/a"), p("/c")],
            "reopening a remembered project should promote it to MRU, not duplicate it"
        );
    }

    #[test]
    fn prepend_bounds_the_list_to_the_maximum() {
        let existing: Vec<PathBuf> = (0..MAX_RECENTS).map(|i| p(&format!("/p{i}"))).collect();

        let out = prepend_recent(&existing, &p("/new"));

        assert_eq!(out.len(), MAX_RECENTS);
        assert_eq!(out[0], p("/new"));
        assert!(
            !out.contains(&p(&format!("/p{}", MAX_RECENTS - 1))),
            "the oldest entry should be evicted once the list is full"
        );
    }

    #[test]
    fn parse_tolerates_a_corrupt_file_as_empty() {
        assert!(parse_recents("this is not json").is_empty());
        assert!(parse_recents("").is_empty());
        assert!(parse_recents("{\"not\": \"an array\"}").is_empty());
    }

    #[test]
    fn parse_and_serialize_round_trip() {
        let recents = vec![p("/one"), p("/two/three")];

        let restored = parse_recents(&serialize_recents(&recents));

        assert_eq!(restored, recents);
    }

    #[test]
    fn most_recent_valid_picks_the_first_entry_that_is_a_real_project() {
        let missing = temp_dir();
        let missing_path = missing.path().join("gone");

        let valid = temp_dir();
        std::fs::create_dir(valid.path().join(".lazyspec")).unwrap();

        let recents = vec![missing_path, valid.path().to_path_buf()];

        assert_eq!(
            most_recent_valid(&recents),
            Some(valid.path().to_path_buf()),
            "a stale head should be skipped in favour of the next valid entry"
        );
    }

    #[test]
    fn most_recent_valid_returns_none_when_no_entry_is_valid() {
        let missing = temp_dir();
        let recents = vec![
            missing.path().join("gone"),
            missing.path().join("also-gone"),
        ];

        assert_eq!(
            most_recent_valid(&recents),
            None,
            "with no valid entry the caller must fall through to the picker"
        );
    }

    #[test]
    fn most_recent_valid_on_an_empty_list_is_none() {
        assert_eq!(most_recent_valid(&[]), None);
    }
}
