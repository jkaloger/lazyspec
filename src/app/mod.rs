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
//! ## Bin-vs-`main.rs` decision (RFC-054 open interface item)
//!
//! The RFC left open whether the app entry lives in a separate
//! `src/bin/lazyspec-app.rs` or a `#[cfg(feature = "app")]` branch in
//! `main.rs`. Decision: expose `app::run()` from the library and let a thin
//! entry call it, rather than baking `main` logic into this module. STORY-185
//! only needs the reusable `run()` seam; wiring an actual binary/menu entry is
//! deferred with the rest of the product surface (STORY-186+). Keeping `run()`
//! library-side means the future entry (bin or `main.rs` branch) is a one-line
//! call and the choice is not locked in here.

pub mod protocol;

#[cfg(feature = "app")]
use std::sync::Arc;

#[cfg(feature = "app")]
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(feature = "app")]
use crate::engine::config::Config;
#[cfg(feature = "app")]
use crate::engine::issue_map::IssueMap;
#[cfg(feature = "app")]
use crate::engine::store::Store;
#[cfg(feature = "app")]
use crate::web::server::{router, AppState};

/// The custom URI scheme the webview navigates to. Every request the WKWebView
/// issues under this scheme is handed to the protocol handler and driven through
/// the axum router. `lazyspec://localhost/` is the page load; `lazyspec://
/// localhost/static/...` are the assets.
#[cfg(feature = "app")]
const SCHEME: &str = "lazyspec";

/// Build the shared [`AppState`] for a project root, mirroring how
/// `lazyspec serve` constructs it in `main.rs` (STORY-185 AC8): load the store
/// into an `Arc<Store>`, resolve GitHub deep-link coords (deep-links disabled
/// when unresolvable), load the issue map, and derive the header repo/branch
/// chips. No socket is involved.
#[cfg(feature = "app")]
fn build_state(root: &std::path::Path) -> anyhow::Result<AppState> {
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
    Ok(AppState {
        store,
        config: Arc::new(config),
        coords,
        issue_map,
        repo_name,
        branch,
    })
}

/// Entry point for the native app (RFC-054 `app::run`).
///
/// Loads a **hardcoded** project path (STORY-185 out-of-scope: picker/recents
/// arrive in STORY-186), builds the router state, and starts Tauri. An
/// app-owned multi-thread tokio runtime services the custom-scheme protocol
/// handler; the handler adapts each webview `http::Request` through the axum
/// `Router` and returns the `http::Response`, so no route is reimplemented and
/// no port is bound.
#[cfg(feature = "app")]
pub fn run() -> anyhow::Result<()> {
    // Hardcoded per STORY-185 (folder picker is STORY-186). The current working
    // directory is the lazyspec project to render.
    let root = std::env::current_dir()?;
    let state = build_state(&root)?;

    // The app owns its tokio runtime, distinct from Tauri's AppKit event loop
    // (AC7). The protocol handler blocks on this runtime to service each
    // request; building the Router per request is cheap (it is a clone-able
    // `tower::Service` of shared `Arc` state).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let runtime = Arc::new(runtime);

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol(SCHEME, move |ctx, request, responder| {
            // Clone the shared `AppState` (Arc-backed) and rebuild the router as a
            // fresh `tower::Service` per request. `respond` may be called from any
            // thread, so service the request on the app runtime off the caller
            // thread and respond when the future resolves.
            let app_router = router(ctx.app_handle().state::<AppState>().inner().clone());
            let runtime = runtime.clone();
            std::thread::spawn(move || {
                let response = runtime.block_on(protocol::handle(app_router, request));
                responder.respond(response);
            });
        })
        .setup(move |app| {
            app.manage(state);
            let url = WebviewUrl::CustomProtocol(
                format!("{SCHEME}://localhost/")
                    .parse()
                    .expect("static scheme URL is valid"),
            );
            WebviewWindowBuilder::new(app, "main", url)
                .title("lazyspec")
                .inner_size(1100.0, 800.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("tauri run error: {e}"))
}
