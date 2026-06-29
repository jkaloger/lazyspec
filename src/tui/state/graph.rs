//! Graph forest flattening and sibling ordering now live in
//! [`crate::engine::graph`] (RFC-052 / STORY-179) so the TUI and the web
//! `/graph` view share one ordering implementation. This module re-exports them
//! so existing `super::graph::` paths in the TUI keep working unchanged.

pub use crate::engine::graph::{flatten_forest, GraphSort};
