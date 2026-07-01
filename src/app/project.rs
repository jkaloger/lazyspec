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
    let store = Arc::new(Store::load(root, &config)?);
    let coords = crate::engine::github_url::resolve_repo_coords(&config, root);
    let issue_map = Arc::new(IssueMap::load(root).unwrap_or_default());
    let repo_name = store
        .root()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let branch = crate::engine::git_status::query_git_branch(store.root());
    Ok(crate::web::server::AppState {
        store,
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
            state.store.root().file_name(),
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
}
