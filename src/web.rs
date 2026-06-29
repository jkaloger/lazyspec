//! Read-only web view for lazyspec documents (RFC-052).
//!
//! This layer is gated behind the `web` cargo feature so the async stack
//! (tokio/axum/askama) never enters default builds. Per convention principle 3
//! it imports only from [`crate::engine`], never from `cli` or `tui`.

pub mod render;
pub mod routes;
pub mod server;

pub use server::serve;
