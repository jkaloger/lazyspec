//! Axum server wiring: build the router over a shared [`AppState`] and bind
//! it to loopback. Imports only from [`crate::engine`].

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use axum::routing::get;
use axum::Router;

use crate::engine::github_url::RepoCoords;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::web::{assets, routes};

/// The default loopback port for `lazyspec serve` (RFC-052 / STORY-176).
pub const DEFAULT_PORT: u16 = 8787;

/// A swappable holder for the router's shared [`Store`]. The inner `Arc<Store>`
/// is the current live store; the reload loop replaces it wholesale (see
/// [`SharedStore::swap`]) while requests read a cheap per-request snapshot via
/// [`SharedStore::snapshot`].
///
/// A `RwLock<Arc<Store>>` (rather than a bare `Arc<Store>`) gives interior
/// mutability without a new crate: `arc-swap` is not vendored and crates.io is
/// off the sandbox network. The lock is held only long enough to clone the inner
/// `Arc` (a refcount bump) and is released before the request does any work, so
/// no lock is ever held across a request and a swap is visible only to
/// subsequent requests.
#[derive(Clone)]
pub struct SharedStore(Arc<RwLock<Arc<Store>>>);

impl SharedStore {
    /// Wrap an initial store as the shared, swappable holder.
    pub fn new(store: Store) -> Self {
        SharedStore(Arc::new(RwLock::new(Arc::new(store))))
    }

    /// Clone the current inner `Arc<Store>` and release the read lock
    /// immediately, giving the caller a consistent snapshot for the whole
    /// request with no lock held across it.
    pub fn snapshot(&self) -> Arc<Store> {
        Arc::clone(&self.0.read().expect("store lock poisoned"))
    }

    /// Atomically replace the inner store. Visible only to snapshots taken after
    /// this returns; in-flight requests keep the snapshot they already cloned.
    pub fn swap(&self, store: Store) {
        *self.0.write().expect("store lock poisoned") = Arc::new(store);
    }
}

/// Shared, read-only application state behind the router. Carries the loaded
/// store plus the GitHub deep-link inputs resolved once at startup: the repo
/// `coords` (`None` when unresolvable, which disables deep-links) and the
/// `issue_map` used to construct issue/milestone URLs.
#[derive(Clone)]
pub struct AppState {
    pub store: SharedStore,
    pub config: Arc<crate::engine::config::Config>,
    pub coords: Option<RepoCoords>,
    pub issue_map: Arc<IssueMap>,
    /// The store root directory name, shown in the header repo chip.
    pub repo_name: String,
    /// The current git branch, shown in the header chip after the separator.
    pub branch: Option<String>,
}

/// Resolve the port to bind: the explicit `--port` value, else [`DEFAULT_PORT`].
pub fn resolve_port(port: Option<u16>) -> u16 {
    port.unwrap_or(DEFAULT_PORT)
}

/// Build the application router with the shared state. Extracted from [`serve`]
/// so tests can drive it via `tower::ServiceExt::oneshot` without binding a real
/// socket.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::list_page))
        .route("/fragment/list", get(routes::list_fragment))
        .route("/search", get(routes::search))
        .route("/graph", get(routes::graph))
        .route("/doc/{id}", get(routes::doc_page))
        .route("/static/lazyspec.css", get(assets::stylesheet))
        .route("/static/fonts/{name}", get(assets::font))
        .with_state(state)
}

/// Start the HTTP server bound to `127.0.0.1:<port>` (default [`DEFAULT_PORT`]),
/// serving the read-only document view backed by `state`. Logs the bound
/// address. Blocks until the server stops.
pub fn serve(state: AppState, port: Option<u16>) -> anyhow::Result<()> {
    let port = resolve_port(port);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(serve_async(state, port))
}

async fn serve_async(state: AppState, port: u16) -> anyhow::Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!("lazyspec serve listening on http://{bound}");

    // Start the live-reload loop against the served root, holding its handle for
    // the server's lifetime so edits swap the router's shared store on the next
    // request (STORY-187 AC7). The handle is dropped when `serve_async` returns.
    let _watch = start_reload_watch(&state);

    axum::serve(listener, router(state)).await?;
    Ok(())
}

/// Start the live-reload watch for a served `state`, returning its handle for the
/// caller to hold for the server's lifetime. The watch root is the served store's
/// root, and the watch swaps `state.store` on relevant edits via the shared
/// `web::watch` seam. A watcher that fails to start is non-fatal — `serve` keeps
/// running without live reload (matching the failed-reload resilience stance) —
/// so this returns `None` rather than propagating. Extracted from [`serve_async`]
/// so the serve-side wiring (root derivation + watch start over the router's
/// shared store) is testable without binding a socket.
fn start_reload_watch(state: &AppState) -> Option<crate::web::watch::WatchHandle> {
    let root = state.store.snapshot().root().to_path_buf();
    match crate::web::watch::watch(&root, &state.config, state.store.clone()) {
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("lazyspec serve: live reload disabled (watch failed to start: {e})");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, StoreBackend, TypeDef};
    use std::path::Path;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    fn write_project(root: &Path) -> Config {
        let mut config = Config::default();
        let mut t = TypeDef::test_fixture("doc", StoreBackend::Filesystem);
        t.dir = "docs/doc".to_string();
        config.documents.types = vec![t];
        std::fs::write(root.join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();
        std::fs::create_dir_all(root.join("docs/doc")).unwrap();
        Config::load(root, &crate::engine::fs::RealFileSystem).unwrap()
    }

    fn write_doc(root: &Path, name: &str, title: &str) {
        std::fs::write(
            root.join("docs/doc").join(name),
            format!(
                "---\ntitle: \"{title}\"\ntype: doc\nstatus: draft\nauthor: \"a\"\ndate: 2026-07-01\ntags: []\n---\n\nbody\n"
            ),
        )
        .unwrap();
    }

    fn app_state(root: &Path, config: Config) -> AppState {
        AppState {
            store: SharedStore::new(Store::load(root, &config).unwrap()),
            config: Arc::new(config),
            coords: None,
            issue_map: Arc::new(crate::engine::issue_map::IssueMap::default()),
            repo_name: "testrepo".into(),
            branch: None,
        }
    }

    // Serve wiring (ITERATION-256 task 1): starting the reload watch for a served
    // state succeeds and derives the watch root from the served store, and the
    // handle drops cleanly. Uses the production `recommended_watcher` path, which
    // starts fine under the sandbox (only FSEvents *delivery* is unavailable).
    #[test]
    fn start_reload_watch_starts_for_served_state_and_drops_cleanly() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = write_project(root);
        let state = app_state(root, config);

        let handle = start_reload_watch(&state);

        assert!(
            handle.is_some(),
            "serve must start a live-reload watch for a valid served root"
        );
        drop(handle);
    }

    // Serve wiring drives the router's shared store (ITERATION-256 task 1 / AC7):
    // the watch that serve starts targets `state.store` — the exact `SharedStore`
    // the router reads — so an edit under the served root swaps that store and the
    // next router snapshot reflects it. Delivery is driven through a `PollWatcher`
    // over the served state's own store because FSEvents is unavailable under the
    // sandbox; the swap target (`state.store`) is identical to production `serve`.
    #[test]
    fn served_shared_store_reflects_edit_via_reload_watch() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = write_project(root);
        write_doc(root, "DOC-001-alpha.md", "Alpha");
        let state = app_state(root, config);

        assert_eq!(state.store.snapshot().all_docs().len(), 1);

        let poll_config = notify::Config::default().with_poll_interval(Duration::from_millis(50));
        let watch_root = state.store.snapshot().root().to_path_buf();
        let _handle = crate::web::watch::watch_with(
            &watch_root,
            &state.config,
            state.store.clone(),
            move |handler| notify::PollWatcher::new(handler, poll_config),
        )
        .expect("poll watch should start");

        write_doc(root, "DOC-002-beta.md", "Beta");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let n = state.store.snapshot().all_docs().len();
            if n == 2 || Instant::now() >= deadline {
                assert_eq!(
                    n, 2,
                    "the router's shared store must reflect the served edit"
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
