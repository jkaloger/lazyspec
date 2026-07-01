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

/// A running watch loop, and the single stop/replace seam both hosted `serve`
/// and the app's project switch use (ITERATION-256 task 3). Holds the live
/// `notify` watcher (boxed so the watcher backend is an implementation detail)
/// and the worker thread's join handle.
///
/// **Stop semantics:** dropping the handle drops the watcher — which drops the
/// event sender — so the worker's `for event in rx` loop ends and the thread
/// finishes. There is no explicit `stop()`; `drop(handle)` is the stop.
///
/// **Replace semantics:** to re-point at a new root, the owner drops the old
/// handle and calls [`watch`] again against the new root's [`SharedStore`]. The
/// app shell holds the handle in a slot it overwrites on switch (see
/// `crate::app::switch_project`); `serve` holds a single handle for the server's
/// lifetime.
pub struct WatchHandle {
    _watcher: Box<dyn Watcher + Send>,
    _worker: JoinHandle<()>,
}

/// Start watching `root`'s watch set and swapping `shared` on relevant changes.
///
/// Builds a `notify` watcher over [`crate::engine::watch::watch_paths`] that
/// forwards events on a channel to a dedicated worker thread; the worker runs
/// [`reload_and_swap`] on each relevant event. The watcher and worker are owned
/// by the returned [`WatchHandle`] (web owns its own thread, per RFC-054).
///
/// Uses `notify`'s platform-recommended backend (FSEvents on macOS). Tests that
/// must assert live event delivery use [`watch_with`] with a `PollWatcher`,
/// because FSEvents is unavailable under sandboxed/emulated environments (per
/// `notify`'s own docs); the reload/swap wiring exercised is identical.
pub fn watch(root: &Path, config: &Config, shared: SharedStore) -> notify::Result<WatchHandle> {
    watch_with(root, config, shared, notify::recommended_watcher)
}

/// [`watch`] parameterized over how the `notify::Watcher` is constructed, so a
/// test can inject a `PollWatcher` where the recommended (FSEvents) backend is
/// unavailable. The watch-set registration, event-forwarding channel, worker
/// loop, and [`reload_and_swap`] call are identical to [`watch`]; only the
/// watcher backend differs. Not part of the public seam — `watch` is.
pub(crate) fn watch_with<W, F>(
    root: &Path,
    config: &Config,
    shared: SharedStore,
    make_watcher: F,
) -> notify::Result<WatchHandle>
where
    W: Watcher + Send + 'static,
    F: FnOnce(Box<dyn FnMut(notify::Result<notify::Event>) + Send + 'static>) -> notify::Result<W>,
{
    let (tx, rx) = std::sync::mpsc::channel::<notify::Event>();

    let handler = move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    };
    let mut watcher = make_watcher(Box::new(handler))?;
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
        _watcher: Box::new(watcher),
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

    // Build a `PollWatcher` with a short poll interval. The recommended (FSEvents)
    // backend is unavailable under the sandbox (per `notify`'s own docs), so the
    // live-delivery tests inject a poll watcher through `watch_with`; the reload/
    // swap wiring under test is identical to production `watch`.
    fn start_poll_watch(root: &Path, config: &Config, shared: SharedStore) -> WatchHandle {
        let poll_config =
            notify::Config::default().with_poll_interval(std::time::Duration::from_millis(50));
        watch_with(root, config, shared, move |handler| {
            notify::PollWatcher::new(handler, poll_config)
        })
        .expect("poll watch should start")
    }

    // Poll `shared` until it holds `expected` docs or `timeout` elapses, returning
    // the final observed count. Filesystem-event delivery is asynchronous, so the
    // live-watch tests below assert against this bounded wait rather than a fixed
    // sleep.
    fn wait_for_doc_count(
        shared: &SharedStore,
        expected: usize,
        timeout: std::time::Duration,
    ) -> usize {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let count = shared.snapshot().all_docs().len();
            if count == expected || std::time::Instant::now() >= deadline {
                return count;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    // AC7 (hosted serve path, driven at the web layer): with a live `watch` over a
    // served root, creating a doc under `docs/` swaps the shared store so the next
    // snapshot (what the next request would read) reflects the edit.
    #[test]
    fn live_watch_reflects_edit_under_served_root_on_next_snapshot() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let config = write_project(root);
        write_doc(root, "DOC-001-alpha.md", "Alpha");

        let shared = SharedStore::new(Store::load(root, &config).unwrap());
        let _handle = start_poll_watch(root, &config, shared.clone());
        assert_eq!(shared.snapshot().all_docs().len(), 1);

        write_doc(root, "DOC-002-beta.md", "Beta");

        let count = wait_for_doc_count(&shared, 2, std::time::Duration::from_secs(5));
        assert_eq!(
            count, 2,
            "an edit under the served root must be reflected on the next snapshot"
        );
    }

    // AC5 (switch re-points the watcher, driven at the web layer): after replacing
    // the watch handle to point at a new root, edits to the new root drive reloads
    // of its store while edits to the old root no longer do. Modelling the switch
    // as "drop the old handle, start a new one over a fresh SharedStore" mirrors
    // what the app shell does on `switch_project`.
    #[test]
    fn switch_repoints_watch_to_new_root_and_drops_old() {
        let tmp_a = TempDir::new().unwrap();
        let root_a = tmp_a.path();
        let config_a = write_project(root_a);
        write_doc(root_a, "DOC-001-a.md", "A one");

        let shared_a = SharedStore::new(Store::load(root_a, &config_a).unwrap());
        let handle_a = start_poll_watch(root_a, &config_a, shared_a.clone());

        let tmp_b = TempDir::new().unwrap();
        let root_b = tmp_b.path();
        let config_b = write_project(root_b);
        write_doc(root_b, "DOC-001-b.md", "B one");

        // Switch: build fresh state for B and re-point the watch. Dropping the old
        // handle stops A's watcher.
        let shared_b = SharedStore::new(Store::load(root_b, &config_b).unwrap());
        drop(handle_a);
        let _handle_b = start_poll_watch(root_b, &config_b, shared_b.clone());

        assert_eq!(shared_b.snapshot().all_docs().len(), 1);

        // An edit under the NEW root drives a reload of B's store.
        write_doc(root_b, "DOC-002-b.md", "B two");
        let b_count = wait_for_doc_count(&shared_b, 2, std::time::Duration::from_secs(5));
        assert_eq!(
            b_count, 2,
            "edits to the new root must drive reloads after the switch"
        );

        // An edit under the OLD root must NOT drive a reload: A's handle is dropped,
        // so A's SharedStore stays at its pre-switch snapshot. Give the (now-stopped)
        // watcher ample time to prove no swap fires.
        let a_before = shared_a.snapshot().all_docs().len();
        write_doc(root_a, "DOC-002-a.md", "A two");
        let a_after = wait_for_doc_count(
            &shared_a,
            a_before + 1,
            std::time::Duration::from_millis(500),
        );
        assert_eq!(
            a_after, a_before,
            "edits to the old root must not drive reloads after the watch is re-pointed"
        );
    }
}
