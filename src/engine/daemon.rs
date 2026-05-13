//! Daemon engine — long-running background process per workspace.
//!
//! Binds a Unix domain socket at `<root>/.lazyspec/daemon.sock` for IPC, then
//! blocks on a shutdown channel. On shutdown it stops the accept loop, releases
//! all leases owned by this host, unlinks the socket, and returns.
//!
//! Single-instance enforcement is via the connect-probe: if the socket exists
//! and accepts a connection, another daemon is live and this one bails. If the
//! socket file exists but no listener is accepting (`ECONNREFUSED`) or the file
//! is missing (`ENOENT`), the stale file is removed and binding proceeds.

use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{bounded, Receiver};

use super::config::Config;
use super::git_ref::GitCli;
use super::host_id;
use super::lease::LeaseEngine;
use super::runner::ClaudeP;
use super::tick::{EngineLeaseOps, GitWorktreeProvisioner, SystemClock, TickLoop, TickRunner};

const SOCKET_REL_PATH: &str = ".lazyspec/daemon.sock";
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Releases leases owned by a given host prefix. Trait seam for testability;
/// the production impl wraps [`LeaseEngine::release_by_host_prefix`].
pub trait LeaseReleaser: Send + Sync {
    fn release_host_leases(&self, host_prefix: &str) -> Result<()>;
}

/// Production [`LeaseReleaser`] backed by a real [`LeaseEngine`] over [`GitCli`].
pub struct RealLeaseReleaser {
    engine: LeaseEngine<GitCli>,
    root: PathBuf,
    type_names: Vec<String>,
}

impl RealLeaseReleaser {
    pub fn new(engine: LeaseEngine<GitCli>, root: PathBuf, type_names: Vec<String>) -> Self {
        Self {
            engine,
            root,
            type_names,
        }
    }
}

impl LeaseReleaser for RealLeaseReleaser {
    fn release_host_leases(&self, host_prefix: &str) -> Result<()> {
        let type_refs: Vec<&str> = self.type_names.iter().map(|s| s.as_str()).collect();
        let released = self
            .engine
            .release_by_host_prefix(&self.root, &type_refs, host_prefix)?;
        eprintln!("released {} host-owned leases", released.len());
        Ok(())
    }
}

/// Per-workspace daemon. Holds its socket path, host identity, a lease
/// releaser used to clean up on graceful shutdown, and an optional
/// orchestrator tick runner that runs on its own thread.
pub struct Daemon {
    pub root: PathBuf,
    pub sock_path: PathBuf,
    pub host_id: String,
    pub lease_releaser: Box<dyn LeaseReleaser>,
    pub tick_runner: Option<Box<dyn TickRunner>>,
}

impl Daemon {
    /// Build a [`Daemon`] from workspace root + config.
    ///
    /// Requires `config.coordination` to be present (the daemon's whole purpose
    /// is lease management). Returns an error otherwise.
    pub fn new(root: &Path, config: &Config) -> Result<Self> {
        let coordination = config
            .coordination
            .clone()
            .ok_or_else(|| anyhow!("daemon requires [coordination] config section"))?;
        let host_id = host_id::host_id(root)?;
        let sock_path = root.join(SOCKET_REL_PATH);
        let type_names: Vec<String> = config
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let engine = LeaseEngine::new(GitCli, coordination.clone());
        let releaser = RealLeaseReleaser::new(engine, root.to_path_buf(), type_names);

        let tick_runner: Option<Box<dyn TickRunner>> =
            if let Some(orch) = config.orchestration.as_ref() {
                let runner = ClaudeP {
                    binary: orch.runtime.claude_binary.clone(),
                    allowed_tools: orch.runtime.allowed_tools.clone(),
                    turn_timeout_ms: orch.runtime.turn_timeout_ms,
                };
                let lease_engine = LeaseEngine::new(GitCli, coordination);
                let lease_ops = EngineLeaseOps {
                    engine: lease_engine,
                    root: root.to_path_buf(),
                };
                let tl = TickLoop::new(
                    root.to_path_buf(),
                    config.clone(),
                    host_id.clone(),
                    runner,
                    GitCli,
                    lease_ops,
                    SystemClock,
                    GitWorktreeProvisioner,
                );
                Some(Box::new(tl))
            } else {
                None
            };

        Ok(Self {
            root: root.to_path_buf(),
            sock_path,
            host_id,
            lease_releaser: Box::new(releaser),
            tick_runner,
        })
    }

    /// Test/integration-only ctor that injects a custom [`LeaseReleaser`].
    pub fn with_lease_releaser(
        root: PathBuf,
        sock_path: PathBuf,
        host_id: String,
        lease_releaser: Box<dyn LeaseReleaser>,
    ) -> Self {
        Self {
            root,
            sock_path,
            host_id,
            lease_releaser,
            tick_runner: None,
        }
    }

    /// Test/integration-only ctor that injects both a custom [`LeaseReleaser`]
    /// and a [`TickRunner`]. `None` for `tick_runner` skips spawning the tick
    /// thread (mirrors the production "no `[orchestration]` config" path).
    pub fn with_tick_runner(
        root: PathBuf,
        sock_path: PathBuf,
        host_id: String,
        lease_releaser: Box<dyn LeaseReleaser>,
        tick_runner: Option<Box<dyn TickRunner>>,
    ) -> Self {
        Self {
            root,
            sock_path,
            host_id,
            lease_releaser,
            tick_runner,
        }
    }

    /// Run the daemon in the foreground. Blocks until `shutdown_rx` receives a
    /// signal (or the channel is closed).
    ///
    /// On shutdown: stops the accept loop, releases this host's leases, unlinks
    /// the socket file, and returns `Ok(())`. If another daemon is already
    /// listening on the socket, returns an error immediately.
    pub fn run(&mut self, shutdown_rx: Receiver<()>) -> Result<()> {
        let listener = bind_listener(&self.sock_path)?;
        listener
            .set_nonblocking(true)
            .context("failed to set listener non-blocking")?;

        let running = Arc::new(AtomicBool::new(true));
        let accept_running = Arc::clone(&running);
        let accept_handle = thread::spawn(move || accept_loop(listener, accept_running));

        // Spawn tick thread if orchestration is wired in.
        let (tick_tx, tick_handle) = match self.tick_runner.take() {
            Some(tr) => {
                let (tx, rx) = bounded::<()>(1);
                let handle = thread::spawn(move || tr.run(rx));
                (Some(tx), Some(handle))
            }
            None => (None, None),
        };

        // Block until shutdown. recv() returning Err means the sender was
        // dropped — treat that as a shutdown too.
        let _ = shutdown_rx.recv();

        // Order: stop accept loop flag -> signal & join tick -> join accept ->
        // release leases (backstop) -> unlink socket.
        running.store(false, Ordering::SeqCst);

        if let Some(tx) = tick_tx {
            // bounded(1); send is best-effort because the tick may already
            // have stopped (e.g. internal error path).
            let _ = tx.send(());
        }
        if let Some(handle) = tick_handle {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("warning: tick loop returned error: {e}"),
                Err(_) => eprintln!("warning: tick thread panicked"),
            }
        }

        // Best-effort drain: v1 has no inflight tracking. Just join the accept
        // thread so we don't tear down state from under it.
        // drain hook: later slices add inflight tracking
        let _ = accept_handle.join();

        if let Err(e) = self.lease_releaser.release_host_leases(&self.host_id) {
            eprintln!("warning: failed to release host leases on shutdown: {e}");
        }

        match fs::remove_file(&self.sock_path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!(
                    "warning: failed to unlink socket {}: {e}",
                    self.sock_path.display()
                );
            }
        }

        Ok(())
    }
}

/// Probe-then-bind. If a live listener answers on `sock_path`, bail. Otherwise
/// remove any stale file at the path and bind a fresh listener.
fn bind_listener(sock_path: &Path) -> Result<UnixListener> {
    match UnixStream::connect(sock_path) {
        Ok(_) => {
            return Err(anyhow!(
                "another daemon is already listening on {}",
                sock_path.display()
            ));
        }
        Err(e) => match e.kind() {
            io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound => {}
            _ => {
                // Treat any other connect error (e.g. ENOTSOCK from a regular
                // file at the path) as "not a live daemon"; we'll attempt to
                // clear and rebind.
            }
        },
    }

    if let Some(parent) = sock_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create socket parent dir: {}", parent.display()))?;
    }

    match fs::remove_file(sock_path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow::Error::new(e).context(format!(
                "failed to remove stale socket file: {}",
                sock_path.display()
            )));
        }
    }

    UnixListener::bind(sock_path)
        .with_context(|| format!("failed to bind unix socket at {}", sock_path.display()))
}

fn accept_loop(listener: UnixListener, running: Arc<AtomicBool>) {
    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((_stream, _addr)) => {
                // v1: no protocol yet. Drop the stream; future slices will
                // route the connection to a handler.
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(e) => {
                eprintln!("warning: accept error: {e}");
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;
    use tempfile::TempDir;

    /// Records every `release_host_leases` call.
    struct RecordingReleaser {
        calls: Mutex<Vec<String>>,
    }

    impl RecordingReleaser {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl LeaseReleaser for Arc<RecordingReleaser> {
        fn release_host_leases(&self, host_prefix: &str) -> Result<()> {
            self.calls.lock().unwrap().push(host_prefix.to_string());
            Ok(())
        }
    }

    fn wait_for_socket(path: &Path, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if path.exists() && UnixStream::connect(path).is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn make_daemon(td: &TempDir, releaser: Arc<RecordingReleaser>) -> Daemon {
        let root = td.path().to_path_buf();
        fs::create_dir_all(root.join(".lazyspec")).unwrap();
        let sock_path = root.join(SOCKET_REL_PATH);
        Daemon::with_lease_releaser(
            root,
            sock_path,
            "host-test:0000".to_string(),
            Box::new(releaser),
        )
    }

    #[test]
    fn daemon_run_binds_socket_then_drains_on_shutdown() {
        let td = TempDir::new().unwrap();
        let releaser = Arc::new(RecordingReleaser::new());
        let mut daemon = make_daemon(&td, Arc::clone(&releaser));
        let sock_path = daemon.sock_path.clone();
        let expected_host = daemon.host_id.clone();

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));

        assert!(
            wait_for_socket(&sock_path, Duration::from_secs(1)),
            "socket never became available"
        );

        tx.send(()).unwrap();

        let join_deadline = Instant::now() + Duration::from_secs(2);
        while !handle.is_finished() && Instant::now() < join_deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(handle.is_finished(), "daemon did not shut down in time");
        let result = handle.join().expect("daemon thread panicked");
        assert!(result.is_ok(), "daemon returned error: {:?}", result);

        assert!(!sock_path.exists(), "socket file should be unlinked");

        let calls = releaser.calls();
        assert_eq!(calls.len(), 1, "expected exactly one release call");
        assert_eq!(calls[0], expected_host);
    }

    #[test]
    fn daemon_replaces_stale_socket_on_startup() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(SOCKET_REL_PATH);
        // Pre-create a regular file at the socket path — connect() will fail.
        fs::write(&sock_path, b"not a socket").unwrap();

        let releaser = Arc::new(RecordingReleaser::new());
        let mut daemon = Daemon::with_lease_releaser(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-test:1111".to_string(),
            Box::new(Arc::clone(&releaser)),
        );

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));

        assert!(
            wait_for_socket(&sock_path, Duration::from_secs(1)),
            "stale socket was not replaced"
        );

        tx.send(()).unwrap();
        let result = handle.join().expect("daemon thread panicked");
        assert!(result.is_ok(), "daemon returned error: {:?}", result);
        assert!(!sock_path.exists());
    }

    #[test]
    fn daemon_replaces_stale_socket_when_no_listener() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(SOCKET_REL_PATH);

        // Bind a listener, then drop it. `UnixListener` drop does NOT unlink on
        // most platforms, so the inode remains but connect() yields ECONNREFUSED.
        {
            let _listener = UnixListener::bind(&sock_path).unwrap();
        }
        assert!(sock_path.exists(), "precondition: socket file present");
        assert!(
            UnixStream::connect(&sock_path).is_err(),
            "precondition: no live listener"
        );

        let releaser = Arc::new(RecordingReleaser::new());
        let mut daemon = Daemon::with_lease_releaser(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-test:2222".to_string(),
            Box::new(Arc::clone(&releaser)),
        );

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));

        assert!(
            wait_for_socket(&sock_path, Duration::from_secs(1)),
            "daemon did not rebind over dead socket"
        );

        tx.send(()).unwrap();
        let result = handle.join().expect("daemon thread panicked");
        assert!(result.is_ok());
    }

    #[test]
    fn daemon_refuses_to_start_when_socket_already_live() {
        let td = TempDir::new().unwrap();
        let releaser_a = Arc::new(RecordingReleaser::new());
        let mut daemon_a = make_daemon(&td, Arc::clone(&releaser_a));
        let sock_path = daemon_a.sock_path.clone();

        let (tx_a, rx_a) = crossbeam_channel::bounded::<()>(1);
        let handle_a = thread::spawn(move || daemon_a.run(rx_a));

        assert!(
            wait_for_socket(&sock_path, Duration::from_secs(1)),
            "daemon A never came up"
        );

        // Daemon B at the same path should refuse.
        let releaser_b = Arc::new(RecordingReleaser::new());
        let mut daemon_b = Daemon::with_lease_releaser(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-test:bbbb".to_string(),
            Box::new(Arc::clone(&releaser_b)),
        );
        let (_tx_b, rx_b) = crossbeam_channel::bounded::<()>(1);
        let result_b = daemon_b.run(rx_b);
        assert!(
            result_b.is_err(),
            "second daemon should refuse, got: {:?}",
            result_b
        );

        // Tear down A.
        tx_a.send(()).unwrap();
        let result_a = handle_a.join().expect("daemon A thread panicked");
        assert!(result_a.is_ok());

        // B's releaser must NOT have been called (it never ran).
        assert!(releaser_b.calls().is_empty());
    }

    #[test]
    fn daemon_release_called_with_host_prefix() {
        let td = TempDir::new().unwrap();
        let releaser = Arc::new(RecordingReleaser::new());
        fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(SOCKET_REL_PATH);
        let mut daemon = Daemon::with_lease_releaser(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-xyz:abcd-1234".to_string(),
            Box::new(Arc::clone(&releaser)),
        );

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));
        assert!(wait_for_socket(&sock_path, Duration::from_secs(1)));
        tx.send(()).unwrap();
        handle.join().unwrap().unwrap();

        let calls = releaser.calls();
        assert_eq!(calls, vec!["host-xyz:abcd-1234".to_string()]);
    }

    // -------- TickRunner wiring (Task 5 / ITERATION-176) --------

    /// Fake [`TickRunner`] that records lifecycle events and shares an
    /// ordering log with the [`LeaseReleaser`] so we can prove the tick
    /// thread finishes BEFORE `release_host_leases` runs.
    struct RecordingTickRunner {
        order: Arc<Mutex<Vec<String>>>,
        started: Arc<AtomicBool>,
        shutdown_received: Arc<AtomicBool>,
    }

    impl TickRunner for RecordingTickRunner {
        fn run(self: Box<Self>, shutdown_rx: Receiver<()>) -> Result<()> {
            self.started.store(true, Ordering::SeqCst);
            self.order.lock().unwrap().push("tick:started".to_string());
            // Block until daemon signals shutdown. Disconnected counts too.
            let _ = shutdown_rx.recv();
            self.shutdown_received.store(true, Ordering::SeqCst);
            self.order.lock().unwrap().push("tick:shutdown".to_string());
            Ok(())
        }
    }

    /// Lease releaser that pushes "release" into the shared order log.
    struct OrderingReleaser {
        order: Arc<Mutex<Vec<String>>>,
    }

    impl LeaseReleaser for OrderingReleaser {
        fn release_host_leases(&self, _host_prefix: &str) -> Result<()> {
            self.order.lock().unwrap().push("release".to_string());
            Ok(())
        }
    }

    #[test]
    fn daemon_run_starts_tick_runner() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(SOCKET_REL_PATH);
        let order = Arc::new(Mutex::new(Vec::<String>::new()));
        let started = Arc::new(AtomicBool::new(false));
        let shutdown_received = Arc::new(AtomicBool::new(false));

        let tick = Box::new(RecordingTickRunner {
            order: Arc::clone(&order),
            started: Arc::clone(&started),
            shutdown_received: Arc::clone(&shutdown_received),
        });
        let releaser = Box::new(OrderingReleaser {
            order: Arc::clone(&order),
        });

        let mut daemon = Daemon::with_tick_runner(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-test:tick".to_string(),
            releaser,
            Some(tick),
        );

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));

        assert!(wait_for_socket(&sock_path, Duration::from_secs(1)));
        // The tick thread should be alive by now. Give it a beat to flip the
        // flag if scheduling was unfair.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !started.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(started.load(Ordering::SeqCst), "tick runner did not start");

        tx.send(()).unwrap();
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn daemon_run_signals_tick_shutdown_before_release() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(SOCKET_REL_PATH);
        let order = Arc::new(Mutex::new(Vec::<String>::new()));
        let started = Arc::new(AtomicBool::new(false));
        let shutdown_received = Arc::new(AtomicBool::new(false));

        let tick = Box::new(RecordingTickRunner {
            order: Arc::clone(&order),
            started: Arc::clone(&started),
            shutdown_received: Arc::clone(&shutdown_received),
        });
        let releaser = Box::new(OrderingReleaser {
            order: Arc::clone(&order),
        });

        let mut daemon = Daemon::with_tick_runner(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-test:tick".to_string(),
            releaser,
            Some(tick),
        );

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));
        assert!(wait_for_socket(&sock_path, Duration::from_secs(1)));
        tx.send(()).unwrap();
        handle.join().unwrap().unwrap();

        assert!(shutdown_received.load(Ordering::SeqCst));

        let seq = order.lock().unwrap().clone();
        let tick_shutdown_idx = seq
            .iter()
            .position(|s| s == "tick:shutdown")
            .expect("tick shutdown not recorded");
        let release_idx = seq
            .iter()
            .position(|s| s == "release")
            .expect("release not recorded");
        assert!(
            tick_shutdown_idx < release_idx,
            "tick must finish before release; got {:?}",
            seq
        );
    }

    #[test]
    fn daemon_run_without_orchestration_skips_tick() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(SOCKET_REL_PATH);
        let order = Arc::new(Mutex::new(Vec::<String>::new()));
        let releaser = Box::new(OrderingReleaser {
            order: Arc::clone(&order),
        });

        let mut daemon = Daemon::with_tick_runner(
            td.path().to_path_buf(),
            sock_path.clone(),
            "host-test:nopick".to_string(),
            releaser,
            None,
        );

        let (tx, rx) = crossbeam_channel::bounded::<()>(1);
        let handle = thread::spawn(move || daemon.run(rx));
        assert!(wait_for_socket(&sock_path, Duration::from_secs(1)));
        tx.send(()).unwrap();
        handle.join().unwrap().unwrap();

        let seq = order.lock().unwrap().clone();
        assert!(
            !seq.iter().any(|s| s.starts_with("tick:")),
            "no tick lifecycle events expected, got {:?}",
            seq
        );
        assert_eq!(seq, vec!["release".to_string()]);
    }
}
