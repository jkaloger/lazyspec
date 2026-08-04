use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::fs_ops;
use crate::engine::ops::resolve::resolve_shorthand_or_path;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, PushOutcome};
use anyhow::{bail, Result};
use std::path::Path;

/// Gate a `--status` change against the type's local lifecycle before it reaches
/// the backend.
///
/// Filesystem- and GitHub-backed types own their transition DAG locally: a
/// target must be an out-edge of the current status (or a no-op to the same
/// status). ClickUp-backed types carry no local edges -- their lifecycle states
/// mirror the bound List's status set and ClickUp enforces its own transition
/// rules (RFC-056 §Status handling) -- so lazyspec applies no gate and lets the
/// raw status string through; ClickUp validates and rejects an illegal target.
fn gate_status_transition(type_def: &TypeDef, current: &str, target: &str) -> Result<()> {
    if type_def.store == StoreBackend::ClickupTasks {
        return Ok(());
    }
    let lifecycle = type_def.effective_lifecycle();
    if current != target && !lifecycle.has_edge(current, target) {
        let allowed = lifecycle.targets_from(current);
        let allowed = if allowed.is_empty() {
            "(none)".to_string()
        } else {
            allowed.join(", ")
        };
        bail!(
            "invalid transition for type \"{}\": no edge from \"{}\" to \"{}\" (allowed targets: {})",
            type_def.name,
            current,
            target,
            allowed
        );
    }
    Ok(())
}

pub fn run(
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
) -> Result<PushOutcome> {
    run_with_config(root, store, doc_path, updates, None)
}

pub fn run_with_config(
    root: &Path,
    store: &Store,
    doc_path: &str,
    updates: &[(&str, &str)],
    config: Option<&Config>,
) -> Result<PushOutcome> {
    if let Some(config) = config {
        let doc = resolve_shorthand_or_path(store, doc_path)?;
        let type_name = doc.doc_type.as_str();
        if let Some(type_def) = config.type_by_name(type_name) {
            if let Some((_, target)) = updates.iter().find(|(k, _)| *k == "status") {
                // A type whose lifecycle is an authority board's columns is gated
                // on the state that board column resolves to, and rejects a value
                // naming no column at all -- both offline, from the cached schema
                // snapshot, before any store (and so any client) is built. That is
                // what makes the rejection reachable with no network.
                let board_state = crate::engine::store_dispatch::resolve_authority_status_write(
                    root, type_def, target,
                )?
                .map(|write| write.state);
                gate_status_transition(
                    type_def,
                    doc.status.as_str(),
                    board_state.as_deref().unwrap_or(target),
                )?;
            }
            // Non-filesystem backends dispatch through the store registry; a new
            // backend routes here by being registered in `build_registry`, not by
            // adding another branch. Filesystem keeps its dedicated `fs_ops` path
            // (it edits the document in place by its original path, not by id).
            if type_def.store != StoreBackend::Filesystem {
                // ClickUp authenticates per write: the registry leaves its token
                // unloaded (to keep registry construction free of keychain I/O),
                // so the write path loads the global credential here -- mirroring
                // the create path -- and dispatches against a token-bearing store.
                // A registry-built (token: None) ClickUp store would fail the
                // write on missing auth.
                if type_def.store == StoreBackend::ClickupTasks {
                    let mut store = crate::engine::store_dispatch::clickup_write_store(
                        root,
                        config,
                        "updating",
                        ClickupHttpClient::new,
                        || LayeredCredentialStore::global().load_clickup_token(),
                    )?;
                    return store.update(type_def, &doc.id, updates);
                }
                let mut registry = crate::engine::store_dispatch::build_registry(root, config);
                return registry
                    .for_type(type_def)?
                    .update(type_def, &doc.id, updates);
            }

            return fs_ops::update_document_with_type(
                root,
                store,
                doc_path,
                updates,
                Some(type_def),
            )
            .map(|_| PushOutcome::Synced);
        }
    }

    fs_ops::update_document(root, store, doc_path, updates).map(|_| PushOutcome::Synced)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A filesystem/GitHub type still gates a status move on its lifecycle edges.
    #[test]
    fn gate_rejects_off_edge_move_for_edge_gated_type() {
        let td = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        // The default test fixture lifecycle carries no draft->accepted edge.
        let err = gate_status_transition(&td, "draft", "accepted").unwrap_err();
        assert!(err.to_string().contains("invalid transition"), "got: {err}");
    }

    // Why the authority resolution has to run BEFORE the gate: the gate compares
    // the target verbatim against the type's states, which a board lifecycle holds
    // lowercased. `--status "In Progress"` therefore only survives the gate as the
    // state the board column resolved to.
    #[test]
    fn gate_rejects_board_display_casing_but_accepts_the_resolved_state() {
        let mut td = TypeDef::test_fixture("ticket", StoreBackend::GithubIssues);
        td.status_authority = Some("PROJECT-7".to_string());
        td.lifecycle = crate::engine::config::Lifecycle {
            states: vec!["ready to start".into(), "in progress".into()],
            edges: vec![],
        };

        assert!(gate_status_transition(&td, "ready to start", "In Progress").is_err());
        gate_status_transition(&td, "ready to start", "in progress").unwrap();
    }

    // A ClickUp-backed type bypasses the local gate entirely: any status target
    // passes, because ClickUp (not lazyspec) owns the transition rules and the
    // derived lifecycle carries no edges (RFC-056 §Status handling).
    #[test]
    fn gate_bypasses_clickup_tasks_status_transition() {
        let td = TypeDef::test_fixture("task", StoreBackend::ClickupTasks);
        // A target that would be off-edge for any local DAG: still allowed.
        gate_status_transition(&td, "open", "in progress").unwrap();
        gate_status_transition(&td, "in progress", "done").unwrap();
    }
}
