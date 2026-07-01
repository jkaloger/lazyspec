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
    axum::serve(listener, router(state)).await?;
    Ok(())
}
