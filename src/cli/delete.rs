use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn run(root: &Path, store: &Store, doc_path: &str) -> Result<()> {
    run_with_config(root, store, doc_path, None)
}

pub fn run_with_config(
    root: &Path,
    store: &Store,
    doc_path: &str,
    config: Option<&Config>,
) -> Result<()> {
    if let Some(config) = config {
        let doc = resolve_shorthand_or_path(store, doc_path)?;
        let type_name = doc.doc_type.as_str();
        if let Some(type_def) = config.type_by_name(type_name) {
            // Non-filesystem backends dispatch through the store registry; a new
            // backend routes here by being registered in `build_registry`.
            // Filesystem falls through to its dedicated `fs_ops` path below.
            if type_def.store != StoreBackend::Filesystem {
                let mut registry = crate::engine::store_dispatch::build_registry(root, config);
                return registry.for_type(type_def)?.delete(type_def, &doc.id);
            }
        }
    }

    crate::engine::fs_ops::delete_document(root, store, doc_path)
}
