use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::ops::resolve::resolve_shorthand_or_path;
use crate::engine::store::Store;
use crate::engine::store_dispatch::DocumentStore;
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
                // ClickUp authenticates per write: the registry leaves its token
                // unloaded (to keep registry construction free of keychain I/O),
                // so the delete path loads the global credential here -- mirroring
                // create/update -- and dispatches against a token-bearing store.
                // A registry-built (token: None) ClickUp store would fail the
                // archive on missing auth.
                if type_def.store == StoreBackend::ClickupTasks {
                    let mut store = crate::engine::store_dispatch::clickup_write_store(
                        root,
                        config,
                        "deleting",
                        ClickupHttpClient::new,
                        || LayeredCredentialStore::global().load_clickup_token(),
                    )?;
                    return store.delete(type_def, &doc.id);
                }
                let mut registry = crate::engine::store_dispatch::build_registry(root, config);
                return registry.for_type(type_def)?.delete(type_def, &doc.id);
            }
        }
    }

    crate::engine::fs_ops::delete_document(root, store, doc_path)
}
