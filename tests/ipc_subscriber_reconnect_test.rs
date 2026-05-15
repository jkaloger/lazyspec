//! Integration tests for `ReconnectingSubscriber` (AC9 of STORY-181).
//!
//! Drives the production subscriber over a real `UnixStream` against an
//! in-process test daemon. Verifies the subscriber reconnects and resubscribes
//! after a daemon restart on the same socket path.

use std::collections::HashMap;
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};
use tempfile::TempDir;

use lazyspec::engine::ipc::broadcaster::Broadcaster;
use lazyspec::engine::ipc::client::{
    BackoffSchedule, ReconnectingSubscriber, SocketConnector, ThreadSleeper,
};
use lazyspec::engine::ipc::handler::handle_connection;
use lazyspec::engine::ipc::protocol::{AgentSnapshot, DaemonMessage};
use lazyspec::engine::ipc::state::{DaemonState, StaticSnapshotProvider};
use lazyspec::engine::runner::AgentEvent;

const ACCEPT_POLL: Duration = Duration::from_millis(20);

struct TestHarness {
    running: Arc<AtomicBool>,
    accept_handle: Option<thread::JoinHandle<()>>,
    broadcaster: Broadcaster,
    /// Live conn streams (clones); shutdown on Drop so the subscriber sees EOF
    /// promptly instead of blocking forever on the kept-alive handler thread.
    conns: Arc<Mutex<Vec<UnixStream>>>,
    _wake_rx: Receiver<()>,
    _cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>>,
    sock_path: PathBuf,
    _td: Option<TempDir>,
}

impl TestHarness {
    fn new_at(sock_path: PathBuf, snapshots: Vec<AgentSnapshot>) -> Self {
        if let Some(parent) = sock_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        // Clean any stale file from a previous bind on this path.
        let _ = std::fs::remove_file(&sock_path);

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
        let conns: Arc<Mutex<Vec<UnixStream>>> = Arc::new(Mutex::new(Vec::new()));
        let conns_for_accept = Arc::clone(&conns);

        let accept_handle = thread::spawn(move || {
            while accept_running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        if let Ok(dup) = stream.try_clone() {
                            conns_for_accept.lock().unwrap().push(dup);
                        }
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
            broadcaster,
            conns,
            _wake_rx: wake_rx,
            _cancel_map: cancel_map,
            sock_path,
            _td: None,
        }
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // Force-close any open client conns so handler threads exit and
        // subscribers see EOF.
        if let Ok(mut g) = self.conns.lock() {
            for s in g.drain(..) {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
        if let Some(h) = self.accept_handle.take() {
            let _ = h.join();
        }
        // Make sure the socket path is free for any successor harness.
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

fn sample_event(agent: &str, delta: &str) -> DaemonMessage {
    DaemonMessage::AgentEvent {
        agent_id: agent.into(),
        session_id: "s1".into(),
        event: AgentEvent::Text {
            delta: delta.into(),
        },
    }
}

fn wait_until<F: FnMut() -> bool>(deadline: Duration, mut f: F) -> bool {
    let end = Instant::now() + deadline;
    while Instant::now() < end {
        if f() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    f()
}

fn build_subscriber(sock_path: &Path) -> Receiver<DaemonMessage> {
    let connector = SocketConnector::new(sock_path.to_path_buf());
    let backoff = BackoffSchedule::new(50, 200);
    let sub = ReconnectingSubscriber::new(connector, ThreadSleeper, backoff);
    sub.events()
}

// ---------- AC9: reconnect across restart ----------

#[test]
fn subscriber_resubscribes_after_daemon_restart() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join(".lazyspec/daemon.sock");

    let h1 = TestHarness::new_at(sock.clone(), vec![]);
    let rx = build_subscriber(&sock);

    // Wait for the subscriber to attach to harness #1.
    assert!(
        wait_until(Duration::from_secs(2), || h1.broadcaster.sub_count() >= 1),
        "subscriber never attached to first harness"
    );

    h1.broadcaster.publish(sample_event("a1", "first"));

    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(DaemonMessage::AgentEvent { agent_id, .. }) => assert_eq!(agent_id, "a1"),
        other => panic!("expected first event, got {other:?}"),
    }

    // Restart on the same socket path.
    drop(h1);
    // Brief window for the OS to release the bind; harness Drop also unlinks.
    thread::sleep(Duration::from_millis(50));

    let h2 = TestHarness::new_at(sock.clone(), vec![]);

    assert!(
        wait_until(Duration::from_secs(3), || h2.broadcaster.sub_count() >= 1),
        "subscriber never re-attached after restart"
    );

    h2.broadcaster.publish(sample_event("a2", "second"));

    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(DaemonMessage::AgentEvent { agent_id, .. }) => assert_eq!(agent_id, "a2"),
        other => panic!("expected second event post-reconnect, got {other:?}"),
    }
}

#[test]
fn subscriber_yields_events_post_reconnect() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join(".lazyspec/daemon.sock");

    let h1 = TestHarness::new_at(sock.clone(), vec![]);
    let rx = build_subscriber(&sock);

    // Receive once on h1 to confirm the pipe.
    assert!(
        wait_until(Duration::from_secs(2), || h1.broadcaster.sub_count() >= 1),
        "subscriber never attached"
    );
    h1.broadcaster.publish(sample_event("a", "x"));
    rx.recv_timeout(Duration::from_secs(2))
        .expect("first event delivered");

    drop(h1);
    thread::sleep(Duration::from_millis(50));

    let h2 = TestHarness::new_at(sock.clone(), vec![]);

    // The core assertion: a Subscribe lands on the new daemon, observable as
    // sub_count rising to 1.
    assert!(
        wait_until(Duration::from_secs(3), || h2.broadcaster.sub_count() >= 1),
        "subscriber did not resubscribe after reconnect"
    );

    // And events flow on the new conn.
    h2.broadcaster.publish(sample_event("b", "y"));
    rx.recv_timeout(Duration::from_secs(3))
        .expect("event after reconnect");
}

#[test]
fn subscriber_exits_when_receiver_dropped() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join(".lazyspec/daemon.sock");

    let h = TestHarness::new_at(sock.clone(), vec![]);
    let rx = build_subscriber(&sock);

    assert!(
        wait_until(Duration::from_secs(2), || h.broadcaster.sub_count() >= 1),
        "subscriber never attached"
    );

    drop(rx);

    // Drop alone won't free the broadcaster slot; a publish triggers cleanup
    // when the dead receive-half is detected. Loop a few publishes to be sure.
    let dropped = wait_until(Duration::from_secs(2), || {
        h.broadcaster.publish(sample_event("a", "x"));
        h.broadcaster.sub_count() == 0
    });
    assert!(
        dropped,
        "subscriber thread did not exit after receiver dropped (sub_count={})",
        h.broadcaster.sub_count()
    );
}
