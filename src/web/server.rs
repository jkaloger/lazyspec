//! Axum server wiring: build the router over a shared [`Arc<Store>`] and bind
//! it to loopback. Imports only from [`crate::engine`].

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use crate::engine::store::Store;
use crate::web::routes;

/// The default loopback port for `lazyspec serve` (RFC-052 / STORY-176).
pub const DEFAULT_PORT: u16 = 8787;

/// Resolve the port to bind: the explicit `--port` value, else [`DEFAULT_PORT`].
pub fn resolve_port(port: Option<u16>) -> u16 {
    port.unwrap_or(DEFAULT_PORT)
}

/// Build the application router with the shared store as state. Extracted from
/// [`serve`] so tests can drive it via `tower::ServiceExt::oneshot` without
/// binding a real socket.
pub fn router(store: Arc<Store>) -> Router {
    Router::new()
        .route("/", get(routes::list_page))
        .route("/fragment/list", get(routes::list_fragment))
        .route("/search", get(routes::search))
        .route("/doc/{id}", get(routes::doc_page))
        .with_state(store)
}

/// Start the HTTP server bound to `127.0.0.1:<port>` (default [`DEFAULT_PORT`]),
/// serving the read-only document view backed by `store`. Logs the bound
/// address. Blocks until the server stops.
pub fn serve(store: Arc<Store>, port: Option<u16>) -> anyhow::Result<()> {
    let port = resolve_port(port);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(serve_async(store, port))
}

async fn serve_async(store: Arc<Store>, port: u16) -> anyhow::Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!("lazyspec serve listening on http://{bound}");
    axum::serve(listener, router(store)).await?;
    Ok(())
}
