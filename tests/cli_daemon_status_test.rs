//! Integration tests for `lazyspec daemon status` (ACs 7 + 8 of STORY-181).
//!
//! Drives `cli::daemon::status::run_with_writer` against a real per-test
//! `UnixListener` (spawned in-process by `TestHarness`). The writer seam lets
//! us assert on the rendered payload without spawning a subprocess.

use std::collections::HashMap;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use tempfile::TempDir;

use lazyspec::cli::daemon::status::{run_with_writer, DaemonStatusError};
use lazyspec::engine::ipc::broadcaster::Broadcaster;
use lazyspec::engine::ipc::handler::handle_connection;
use lazyspec::engine::ipc::protocol::AgentSnapshot;
use lazyspec::engine::ipc::state::{DaemonState, StaticSnapshotProvider};

const ACCEPT_POLL: Duration = Duration::from_millis(20);

struct TestHarness {
    running: Arc<AtomicBool>,
    accept_handle: Option<thread::JoinHandle<()>>,
    _broadcaster: Broadcaster,
    _wake_rx: Receiver<()>,
    _cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>>,
    td: TempDir,
}

impl TestHarness {
    fn new(snapshots: Vec<AgentSnapshot>) -> Self {
        let td = TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path: PathBuf = td.path().join(".lazyspec/daemon.sock");

        let broadcaster = Broadcaster::new();
        let cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (wake_tx, wake_rx) = bounded::<()>(1);

        let state = Arc::new(DaemonState {
            cancel_map: Arc::clone(&cancel_map),
            snapshot_provider: Arc::new(StaticSnapshotProvider(snapshots)),
            broadcaster: broadcaster.clone(),
            wake: wake_tx,
        });

        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let accept_running = Arc::clone(&running);
        let accept_state = Arc::clone(&state);

        let accept_handle = thread::spawn(move || {
            while accept_running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let st = Arc::clone(&accept_state);
                        thread::spawn(move || handle_connection(stream, st));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL);
                    }
                    Err(_) => thread::sleep(ACCEPT_POLL),
                }
            }
        });

        Self {
            running,
            accept_handle: Some(accept_handle),
            _broadcaster: broadcaster,
            _wake_rx: wake_rx,
            _cancel_map: cancel_map,
            td,
        }
    }

    fn root(&self) -> &std::path::Path {
        self.td.path()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.accept_handle.take() {
            let _ = h.join();
        }
    }
}

fn snap(agent_id: &str) -> AgentSnapshot {
    AgentSnapshot {
        agent_id: agent_id.into(),
        session_id: format!("{agent_id}-s"),
        doc_id: "STORY-1".into(),
        elapsed_ms: 0,
        tokens_in: 0,
        tokens_out: 0,
    }
}

// ---------- AC7 ----------

#[test]
fn daemon_status_prints_snapshot() {
    let h = TestHarness::new(vec![]);
    let mut buf: Vec<u8> = Vec::new();
    run_with_writer(h.root(), true, &mut buf).expect("status run ok");

    // strip trailing newline before parse
    let trimmed = buf
        .strip_suffix(b"\n")
        .map(|s| s.to_vec())
        .unwrap_or(buf.clone());
    let agents: Vec<AgentSnapshot> =
        serde_json::from_slice(&trimmed).expect("valid AgentSnapshot[] JSON");
    assert!(agents.is_empty(), "expected empty agents, got {agents:?}");
}

#[test]
fn daemon_status_json_shape() {
    let h = TestHarness::new(vec![snap("a1"), snap("a2")]);
    let mut buf: Vec<u8> = Vec::new();
    run_with_writer(h.root(), true, &mut buf).expect("status run ok");

    let trimmed = buf
        .strip_suffix(b"\n")
        .map(|s| s.to_vec())
        .unwrap_or(buf.clone());
    let agents: Vec<AgentSnapshot> = serde_json::from_slice(&trimmed).expect("valid JSON");
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].agent_id, "a1");
    assert_eq!(agents[1].agent_id, "a2");
}

// ---------- AC8 ----------

#[test]
fn daemon_status_absent_daemon() {
    let td = TempDir::new().unwrap();
    let mut buf: Vec<u8> = Vec::new();
    let err = run_with_writer(td.path(), false, &mut buf).expect_err("should fail");
    assert!(
        matches!(err, DaemonStatusError::NotRunning),
        "expected NotRunning, got {err:?}"
    );
    assert!(buf.is_empty(), "no stdout on error, got {buf:?}");
}

#[test]
fn daemon_status_stale_socket_treated_as_absent() {
    let td = TempDir::new().unwrap();
    std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
    std::fs::write(td.path().join(".lazyspec/daemon.sock"), b"not a socket").unwrap();
    let mut buf: Vec<u8> = Vec::new();
    let err = run_with_writer(td.path(), false, &mut buf).expect_err("should fail");
    assert!(
        matches!(err, DaemonStatusError::NotRunning),
        "expected NotRunning, got {err:?}"
    );
    assert!(buf.is_empty());
}
