//! A `notify`-backed live-reload loop for the web view.
//!
//! Watches an opened project's `.lazyspec.toml` and type directories (the set
//! computed by [`crate::engine::watch::watch_paths`]); on a relevant filesystem
//! change it runs [`Store::load`] and, on success, atomically swaps the router's
//! shared store ([`SharedStore`]). A failed load is skipped, so the last-good
//! store keeps serving (STORY-187 AC6).
//!
//! This is new work authored in `web`, not a reuse of the TUI watcher: it has no
//! dependency on `crate::tui`, `AppEvent`, or `App` (principle 3: `app -> web ->
//! engine`, never `web -> tui`). Wiring it into hosted `serve` and re-pointing on
//! project switch are deferred to ITERATION-256.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use notify::{EventKind, RecursiveMode, Watcher};

use crate::engine::config::Config;
use crate::engine::store::Store;
use crate::web::server::SharedStore;

/// Whether a filesystem event should trigger a reload: content-affecting create,
/// modify, or remove events. Access/metadata-only events are ignored so a mere
/// read does not rebuild the store.
pub fn is_relevant(event: &notify::Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// The reload step, isolated from `notify` timing so it is directly testable:
/// rebuild the store from disk and, on success, atomically swap it into `shared`
/// (visible to subsequent requests). On a load error the swap is skipped and the
/// previous store stays live; the returned `bool` reports whether a swap happened
/// (`true` = reloaded, `false` = kept last-good).
pub fn reload_and_swap(root: &Path, config: &Config, shared: &SharedStore) -> bool {
    match Store::load(root, config) {
        Ok(store) => {
            shared.swap(store);
            true
        }
        Err(_) => false,
    }
}

/// A running watch loop. Holds the live `notify` watcher and the worker thread's
/// join handle; dropping it stops the watcher (which drops the event sender and
/// lets the worker thread finish). Owned by the caller so lifetime is explicit.
pub struct WatchHandle {
    _watcher: notify::RecommendedWatcher,
    _worker: JoinHandle<()>,
}

/// Start watching `root`'s watch set and swapping `shared` on relevant changes.
///
/// Builds a `notify` watcher over [`crate::engine::watch::watch_paths`] that
/// forwards events on a channel to a dedicated worker thread; the worker runs
/// [`reload_and_swap`] on each relevant event. The watcher and worker are owned
/// by the returned [`WatchHandle`] (web owns its own thread, per RFC-054).
pub fn watch(root: &Path, config: &Config, shared: SharedStore) -> notify::Result<WatchHandle> {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Event>();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    for path in crate::engine::watch::watch_paths(root, config) {
        watcher.watch(&path, RecursiveMode::NonRecursive)?;
    }

    let worker_root: PathBuf = root.to_path_buf();
    let worker_config = config.clone();
    let worker = std::thread::spawn(move || {
        for event in rx {
            if is_relevant(&event) {
                reload_and_swap(&worker_root, &worker_config, &shared);
            }
        }
    });

    Ok(WatchHandle {
        _watcher: watcher,
        _worker: worker,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, StoreBackend, TypeDef};
    use std::sync::Arc;
    use tempfile::TempDir;

    // A valid config carrying one filesystem doc type, written so it round-trips
    // through strict `Config::load`.
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

    // AC2: a relevant change rebuilds the store and swaps it into the shared
    // holder, so subsequent snapshots observe the new document.
    #[test]
    fn reload_and_swap_rebuilds_and_swaps_on_relevant_change() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = write_project(root);
        write_doc(root, "DOC-001-alpha.md", "Alpha");

        let shared = SharedStore::new(Store::load(root, &config).unwrap());
        assert_eq!(shared.snapshot().all_docs().len(), 1);

        // A new doc lands under docs/ (the change a watcher would observe).
        write_doc(root, "DOC-002-beta.md", "Beta");

        let swapped = reload_and_swap(root, &config, &shared);

        assert!(swapped, "a successful load must swap the shared store");
        assert_eq!(
            shared.snapshot().all_docs().len(),
            2,
            "the swapped store must include the newly-created doc"
        );
    }

    // AC6: when `Store::load` fails, the swap is skipped and the previous
    // last-good store keeps serving.
    #[test]
    fn reload_and_swap_keeps_prior_store_on_load_err() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = write_project(root);
        write_doc(root, "DOC-001-alpha.md", "Alpha");

        let good = Store::load(root, &config).unwrap();
        let shared = SharedStore::new(good);
        let before = shared.snapshot();
        assert_eq!(before.all_docs().len(), 1);

        // Make the type directory unreadable as a directory by replacing it with
        // a regular file, so the next `Store::load` errors (read_dir fails).
        std::fs::remove_dir_all(root.join("docs/doc")).unwrap();
        std::fs::write(root.join("docs/doc"), "not a directory").unwrap();
        assert!(
            Store::load(root, &config).is_err(),
            "precondition: the corrupted type dir must make Store::load fail"
        );

        let swapped = reload_and_swap(root, &config, &shared);

        assert!(!swapped, "a failed load must not swap");
        let after = shared.snapshot();
        assert_eq!(
            after.all_docs().len(),
            1,
            "the last-good store must remain after a failed reload"
        );
        assert!(
            Arc::ptr_eq(&before, &after),
            "the inner Arc must be unchanged after a failed reload"
        );
    }

    // Access/metadata-only events must not trigger a reload; content events must.
    #[test]
    fn is_relevant_matches_content_events_only() {
        use notify::event::{AccessKind, ModifyKind};

        let modify = notify::Event::new(EventKind::Modify(ModifyKind::Any));
        let create = notify::Event::new(EventKind::Create(notify::event::CreateKind::Any));
        let remove = notify::Event::new(EventKind::Remove(notify::event::RemoveKind::Any));
        let access = notify::Event::new(EventKind::Access(AccessKind::Any));

        assert!(is_relevant(&modify));
        assert!(is_relevant(&create));
        assert!(is_relevant(&remove));
        assert!(!is_relevant(&access));
    }

    // Wiring smoke test: `watch` constructs a live watcher over the project's
    // watch set and starts its worker without error. Timing-dependent delivery is
    // covered by the direct `reload_and_swap` tests above, so this asserts only
    // that setup succeeds and the handle drops cleanly.
    #[test]
    fn watch_starts_and_drops_cleanly() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = write_project(root);
        let shared = SharedStore::new(Store::load(root, &config).unwrap());

        let handle = watch(root, &config, shared).expect("watch should start");
        drop(handle);
    }
}
