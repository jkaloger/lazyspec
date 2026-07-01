//! Native macOS desktop shell (RFC-054 / STORY-185): a Tauri window that
//! renders the read-only web view by driving the **existing**
//! [`web::server::router`](crate::web::server::router) in-process over a custom
//! URI scheme, with no TCP port bound.
//!
//! Layering (RFC-054 principle 3): `app -> web -> engine`. This module imports
//! from [`crate::web`] and [`crate::engine`] only, never from `cli`/`tui`.
//!
//! Runtime split (STORY-185 AC7): Tauri runs the AppKit event loop on the main
//! thread; axum/`tower` need a tokio runtime. [`run`] owns a multi-thread tokio
//! runtime for servicing protocol requests, distinct from Tauri's event loop.
//! That boundary is confined to this module.
//!
//! ## Project selection, recents, and switching (STORY-186 / ITERATION-254)
//!
//! On launch [`run`] reopens the most-recent valid remembered project (dropping
//! stale entries), falling back to the picker. A native File menu offers
//! **Open Project…** and a **recents** submenu; both feed [`switch_project`],
//! which rebuilds the router state and reloads the webview. The recents list and
//! its config-dir resolution live in [`project`]; the launch decision, menu, and
//! switch orchestration are the Tauri-facing glue here.
//!
//! ## Bin-vs-`main.rs` decision (RFC-054 open interface item)
//!
//! The RFC left open whether the app entry lives in a separate
//! `src/bin/lazyspec-app.rs` or a `#[cfg(feature = "app")]` branch in
//! `main.rs`. Decision: expose `app::run()` from the library and let a thin
//! entry call it, rather than baking `main` logic into this module.

pub mod project;
pub mod protocol;

#[cfg(feature = "app")]
use std::path::Path;
#[cfg(feature = "app")]
use std::sync::{Arc, RwLock};

#[cfg(feature = "app")]
use tauri::menu::{Menu, MenuEvent, Submenu, SubmenuBuilder};
#[cfg(feature = "app")]
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
#[cfg(feature = "app")]
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

#[cfg(feature = "app")]
use crate::app::project::{run_picker_loop, PickOutcome};
#[cfg(feature = "app")]
use crate::web::server::{router, AppState};

/// The custom URI scheme the webview navigates to. Every request the WKWebView
/// issues under this scheme is handed to the protocol handler and driven through
/// the axum router. `lazyspec://localhost/` is the page load; `lazyspec://
/// localhost/static/...` are the assets.
#[cfg(feature = "app")]
const SCHEME: &str = "lazyspec";

/// The Tauri window/webview label reused for the single app window. The switch
/// flow looks the webview up by this label to reload it after swapping state.
#[cfg(feature = "app")]
const MAIN_WINDOW: &str = "main";

/// The menu id for File > Open Project….
#[cfg(feature = "app")]
const MENU_OPEN_PROJECT: &str = "open-project";

/// Prefix for recents submenu item ids. The remembered path is JSON-encoded
/// after the prefix so the menu-event handler can recover the exact path to
/// switch to without a side table.
#[cfg(feature = "app")]
const MENU_RECENT_PREFIX: &str = "recent:";

/// The router-facing state, held behind a swappable handle so an in-session
/// project switch can atomically replace it (STORY-186 "AppState swapped behind
/// the router"). The protocol handler snapshots the current [`AppState`] (an
/// `Arc`-backed `Clone`) per request; [`switch_project`] takes the write lock
/// and replaces it. Managed on the Tauri app so both the protocol handler and
/// the menu-event handler reach it via [`Manager::state`].
#[cfg(feature = "app")]
#[derive(Clone)]
pub struct SharedState(Arc<RwLock<AppState>>);

#[cfg(feature = "app")]
impl SharedState {
    fn new(state: AppState) -> Self {
        Self(Arc::new(RwLock::new(state)))
    }

    /// Snapshot the current state for building a router. The lock is held only
    /// for the clone; the returned `AppState` is `Arc`-backed so the clone is
    /// cheap and outlives the lock.
    fn snapshot(&self) -> AppState {
        self.0.read().expect("shared state lock poisoned").clone()
    }

    fn replace(&self, state: AppState) {
        *self.0.write().expect("shared state lock poisoned") = state;
    }
}

/// The app-side owner of the live-reload [`WatchHandle`](crate::web::watch::WatchHandle),
/// held so a project switch can stop the old watch and install a new one
/// (STORY-187 AC5, STORY-186 "Switch re-points the watcher"). Managed on the
/// Tauri app alongside [`SharedState`] so [`repoint_watcher`] can reach it from
/// any `AppHandle`.
///
/// Replace semantics: [`WatchGuard::replace`] drops any previous handle before
/// storing the new one, and dropping the previous handle is what stops its
/// watcher (see [`WatchHandle`](crate::web::watch::WatchHandle) — there is no
/// explicit `stop()`; drop is the stop). This is the single stop/replace seam the
/// switch uses, mirroring how `serve` holds one handle for its lifetime.
#[cfg(feature = "app")]
#[derive(Clone)]
pub struct WatchGuard(Arc<std::sync::Mutex<Option<crate::web::watch::WatchHandle>>>);

#[cfg(feature = "app")]
impl WatchGuard {
    fn new() -> Self {
        Self(Arc::new(std::sync::Mutex::new(None)))
    }

    /// Install `handle` as the current watch, dropping (and thereby stopping) any
    /// previously held one first. The old handle is dropped only after the lock
    /// releases so its worker teardown never runs under the guard's lock.
    fn replace(&self, handle: crate::web::watch::WatchHandle) {
        let previous = {
            let mut slot = self.0.lock().expect("watch guard lock poisoned");
            slot.replace(handle)
        };
        drop(previous);
    }
}

/// Entry point for the native app (RFC-054 `app::run`).
///
/// Launch decision (STORY-186): reopen the most-recent remembered project that
/// still validates as a lazyspec project (stale entries are skipped), otherwise
/// drive the native picker. A valid project builds the router state, records the
/// open in recents, installs the File menu, and opens the window. Cancelling the
/// picker with no project to fall back to exits cleanly.
///
/// An app-owned multi-thread tokio runtime services the custom-scheme protocol
/// handler; the handler adapts each webview `http::Request` through the axum
/// `Router` and returns the `http::Response`, so no route is reimplemented and
/// no port is bound.
#[cfg(feature = "app")]
pub fn run() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let runtime = Arc::new(runtime);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(SCHEME, move |ctx, request, responder| {
            // Snapshot the current (swappable) state and rebuild the router as a
            // fresh `tower::Service` per request. `respond` may be called from any
            // thread, so service the request on the app runtime off the caller
            // thread and respond when the future resolves.
            let shared = ctx.app_handle().state::<SharedState>().inner().clone();
            let app_router = router(shared.snapshot());
            let runtime = runtime.clone();
            std::thread::spawn(move || {
                let response = runtime.block_on(protocol::handle(app_router, request));
                responder.respond(response);
            });
        })
        .on_menu_event(|handle, event| on_menu_event(handle, &event))
        .setup(move |app| {
            let handle = app.handle().clone();

            // Launch decision: prefer the most-recent valid remembered project,
            // else drive the picker. `most_recent_valid` drops stale entries so a
            // gone/moved head never opens a broken view (STORY-186).
            let root = match project::most_recent_valid(&project::load_recents()) {
                Some(root) => root,
                None => match pick_project(&handle) {
                    PickOutcome::Selected(root) => root,
                    // Clean exit: nothing to open and the user dismissed the
                    // picker. Never open a window.
                    PickOutcome::Cancelled => {
                        app.handle().exit(0);
                        return Ok(());
                    }
                },
            };

            open_project(&handle, &root)?;

            let url = WebviewUrl::CustomProtocol(
                format!("{SCHEME}://localhost/")
                    .parse()
                    .expect("static scheme URL is valid"),
            );
            WebviewWindowBuilder::new(app, MAIN_WINDOW, url)
                .title("lazyspec")
                .inner_size(1100.0, 800.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("tauri run error: {e}"))
}

/// Drive the launch/File-menu picker loop with the native dialogs. The loop
/// logic (pick -> validate -> re-prompt) lives in [`project::run_picker_loop`]
/// and is unit-tested without Tauri; this only supplies the `blocking_*` dialog
/// closures.
#[cfg(feature = "app")]
fn pick_project(handle: &AppHandle) -> PickOutcome {
    run_picker_loop(
        || {
            handle
                .dialog()
                .file()
                .set_title("Open a lazyspec project")
                .blocking_pick_folder()
                .and_then(|p| p.into_path().ok())
        },
        |message| {
            handle
                .dialog()
                .message(message)
                .kind(MessageDialogKind::Warning)
                .title("Not a lazyspec project")
                .blocking_show();
        },
    )
}

/// Build the router state for `root`, manage it (first open) or swap it
/// (subsequent opens go through [`switch_project`]), record the open in recents,
/// and rebuild the File menu so the recents submenu reflects the new head.
///
/// Called once at launch (before the window exists). Recording failures are
/// non-fatal: a project the user can see must still open even if recents cannot
/// be persisted, so the error is logged and swallowed.
#[cfg(feature = "app")]
fn open_project(handle: &AppHandle, root: &Path) -> anyhow::Result<()> {
    let state = project::build_state(root)?;
    handle.manage(SharedState::new(state));
    handle.manage(WatchGuard::new());

    if let Err(e) = project::record_recent(root) {
        eprintln!("lazyspec: could not record recent project: {e}");
    }
    install_menu(handle)?;

    // Watcher re-point seam (ITERATION-254 task 7 / STORY-187): the launch open
    // establishes the initial watch root over the just-managed router state.
    repoint_watcher(handle, root);
    Ok(())
}

/// Switch the open project to `root` (STORY-186 "Open a different project" /
/// "Switch via recents"): reload the `Store` and rebuild all six `AppState`
/// fields the ITERATION-253 way ([`project::build_state`]), swap it behind the
/// router, reload the webview so it re-fetches `GET /` through the new state,
/// re-point the watcher, record the open in recents, and refresh the menu.
///
/// Invoked from the menu-event handler for both Open Project… and recents. The
/// state swap is atomic under the `SharedState` write lock, so an in-flight
/// protocol request sees either the old or the new state, never a torn one.
#[cfg(feature = "app")]
fn switch_project(handle: &AppHandle, root: &Path) -> anyhow::Result<()> {
    let state = project::build_state(root)?;
    handle.state::<SharedState>().replace(state);

    if let Some(webview) = handle.get_webview_window(MAIN_WINDOW) {
        webview.reload()?;
    }

    // Watcher re-point seam (STORY-187 AC5): stop the previous watch and start a
    // new one over the just-swapped router state so subsequent live-reload
    // observes the new project's files, not the old one's.
    repoint_watcher(handle, root);

    if let Err(e) = project::record_recent(root) {
        eprintln!("lazyspec: could not record recent project: {e}");
    }
    install_menu(handle)?;
    Ok(())
}

/// Re-point the live-reload watcher at `root` (STORY-187 AC5, ITERATION-256
/// task 2). The single call site that hands the current project root to the
/// `web::watch` loop on launch and on every switch.
///
/// Starts a fresh [`web::watch`](crate::web::watch::watch) over `root`'s watch
/// set, targeting the currently-managed router state's [`SharedStore`] — the
/// exact store the protocol handler reads — so a reload's atomic swap is visible
/// to subsequent webview requests. Installing it via [`WatchGuard::replace`]
/// drops the previous handle, which stops the previous project's watcher (drop is
/// the stop; see [`WatchHandle`](crate::web::watch::WatchHandle)). Called after
/// the caller has managed/replaced the [`SharedState`], so the snapshot here is
/// the new project's state.
///
/// A watcher that fails to start is non-fatal: the project stays open and served,
/// just without live reload, matching serve's stance and the failed-reload
/// resilience principle (STORY-187 AC6). The error is logged and swallowed.
#[cfg(feature = "app")]
fn repoint_watcher(handle: &AppHandle, root: &Path) {
    let shared = handle.state::<SharedState>().snapshot();
    match crate::web::watch::watch(root, &shared.config, shared.store.clone()) {
        Ok(watch_handle) => handle.state::<WatchGuard>().replace(watch_handle),
        Err(e) => eprintln!("lazyspec: live reload disabled (watch failed to start: {e})"),
    }
}

/// Handle a File-menu event: **Open Project…** drives the picker then switches;
/// a recents item decodes its path from the item id and switches to it. Switch
/// failures (a project that went missing between menu-build and click) surface a
/// plain-language dialog rather than crashing.
#[cfg(feature = "app")]
fn on_menu_event(handle: &AppHandle, event: &MenuEvent) {
    let id = event.id().as_ref();

    let target = if id == MENU_OPEN_PROJECT {
        match pick_project(handle) {
            PickOutcome::Selected(root) => Some(root),
            PickOutcome::Cancelled => None,
        }
    } else if let Some(encoded) = id.strip_prefix(MENU_RECENT_PREFIX) {
        serde_json::from_str::<std::path::PathBuf>(encoded).ok()
    } else {
        None
    };

    let Some(root) = target else {
        return;
    };

    if let Err(e) = switch_project(handle, &root) {
        handle
            .dialog()
            .message(format!("Couldn’t open that project: {e}"))
            .kind(MessageDialogKind::Warning)
            .title("Couldn’t open project")
            .blocking_show();
    }
}

/// Build the menu and install it as the app menubar (STORY-186 "native File
/// menu"). Starts from Tauri's default menu (the standard macOS app menu with
/// Quit etc.) and appends a File submenu: **Open Project…** plus a recents list
/// sourced from the recents file. Each recents item's id carries the JSON-encoded
/// path so [`on_menu_event`] can recover it.
#[cfg(feature = "app")]
fn install_menu(handle: &AppHandle) -> anyhow::Result<()> {
    let menu = Menu::default(handle)?;
    let file = build_file_submenu(handle, &project::load_recents())?;
    menu.append(&file)?;
    handle.set_menu(menu)?;
    Ok(())
}

/// Build the File submenu with **Open Project…** and one item per recent project
/// (most-recent-first). Factored out so its shape is unit-testable independent of
/// installing it on the live menubar. Recents item ids are `recent:<json-path>`.
#[cfg(feature = "app")]
fn build_file_submenu(
    handle: &AppHandle,
    recents: &[std::path::PathBuf],
) -> anyhow::Result<Submenu<tauri::Wry>> {
    let mut builder = SubmenuBuilder::new(handle, "File").text(MENU_OPEN_PROJECT, "Open Project…");
    if !recents.is_empty() {
        builder = builder.separator();
        for path in recents {
            let id = format!("{MENU_RECENT_PREFIX}{}", serde_json::to_string(path)?);
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            builder = builder.text(id, label);
        }
    }
    Ok(builder.build()?)
}
