//! Integration tests for the per-connection IPC handler. Tests use real
//! `UnixListener` / `UnixStream` and a real `DaemonState`, exercising the
//! newline-JSON framing end-to-end (DICTUM-004).

use std::collections::HashMap;
use std::io::BufReader;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use tempfile::TempDir;

use lazyspec::engine::ipc::broadcaster::Broadcaster;
use lazyspec::engine::ipc::framing::{read_msg, write_msg};
use lazyspec::engine::ipc::handler::handle_connection;
use lazyspec::engine::ipc::protocol::{AgentSnapshot, ClientMessage, DaemonMessage};
use lazyspec::engine::ipc::state::{DaemonState, StaticSnapshotProvider};
use lazyspec::engine::runner::AgentEvent;

const ACCEPT_POLL: Duration = Duration::from_millis(20);

struct TestHarness {
    sock_path: PathBuf,
    running: Arc<AtomicBool>,
    broadcaster: Broadcaster,
    cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>>,
    wake_rx: Receiver<()>,
    accept_handle: Option<thread::JoinHandle<()>>,
    _td: TempDir,
}

impl TestHarness {
    fn new(snapshots: Vec<AgentSnapshot>) -> Self {
        let td = TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(".lazyspec/daemon.sock");

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
            sock_path,
            running,
            broadcaster,
            cancel_map,
            wake_rx,
            accept_handle: Some(accept_handle),
            _td: td,
        }
    }

    fn connect(&self) -> (BufReader<UnixStream>, UnixStream) {
        let stream = UnixStream::connect(&self.sock_path).expect("connect");
        let writer = stream.try_clone().unwrap();
        let reader = BufReader::new(stream);
        (reader, writer)
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

fn sample_event(delta: &str) -> DaemonMessage {
    DaemonMessage::AgentEvent {
        agent_id: "a1".into(),
        session_id: "s1".into(),
        doc_id: "STORY-1".into(),
        event: AgentEvent::Text {
            delta: delta.into(),
        },
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

/// Read a `DaemonMessage` from the socket with a timeout. Spins off a thread
/// to do the blocking read so we don't deadlock the test on a missing
/// response.
fn read_with_timeout(
    reader: &mut BufReader<UnixStream>,
    timeout: Duration,
) -> Option<DaemonMessage> {
    let deadline = Instant::now() + timeout;
    reader
        .get_ref()
        .set_read_timeout(Some(timeout))
        .expect("set_read_timeout");
    let res: anyhow::Result<Option<DaemonMessage>> = read_msg(reader);
    reader.get_ref().set_read_timeout(None).ok();
    let _ = deadline;
    res.ok().flatten()
}

// ---------- AC1: framing on the wire ----------

#[test]
fn framing_over_socket_is_newline_json() {
    let h = TestHarness::new(vec![]);
    let (mut reader, mut writer) = h.connect();

    write_msg(&mut writer, &ClientMessage::Status).unwrap();

    // Raw read one line to validate it ends with '\n' and parses as JSON.
    use std::io::BufRead;
    let mut line = String::new();
    reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let n = reader.read_line(&mut line).unwrap();
    assert!(n > 0);
    assert!(line.ends_with('\n'), "wire form must end in newline");
    let v: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(v["type"], "daemon_status");
}

// ---------- AC2: subscribe ----------

#[test]
fn subscribed_client_receives_published_event() {
    let h = TestHarness::new(vec![]);
    let (mut reader, mut writer) = h.connect();

    write_msg(&mut writer, &ClientMessage::Subscribe).unwrap();

    // Wait for the handler to have processed the subscribe (sub_count > 0)
    // before publishing, else we'd race the publish past the subscription.
    let deadline = Instant::now() + Duration::from_secs(2);
    while h.broadcaster.sub_count() == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.broadcaster.sub_count(), 1);

    let msg = sample_event("hello");
    h.broadcaster.publish(msg.clone());

    let got = read_with_timeout(&mut reader, Duration::from_secs(2))
        .expect("subscribed client must receive event");
    assert_eq!(got, msg);
}

#[test]
fn unsubscribe_stops_delivery() {
    let h = TestHarness::new(vec![]);
    let (mut reader, mut writer) = h.connect();

    write_msg(&mut writer, &ClientMessage::Subscribe).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while h.broadcaster.sub_count() == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    write_msg(&mut writer, &ClientMessage::Unsubscribe).unwrap();
    // Send a follow-up Status so we can synchronize on the handler having
    // processed the Unsubscribe by the time it replies. Drains the reader
    // of the status response too.
    write_msg(&mut writer, &ClientMessage::Status).unwrap();
    let got_status = read_with_timeout(&mut reader, Duration::from_secs(2))
        .expect("status response after unsubscribe");
    assert!(matches!(got_status, DaemonMessage::DaemonStatus { .. }));

    // Broadcaster prunes dead senders on next publish (retain on Err). After
    // pruning, sub_count should be 0.
    h.broadcaster.publish(sample_event("dropped"));
    assert_eq!(h.broadcaster.sub_count(), 0);

    let got = read_with_timeout(&mut reader, Duration::from_millis(300));
    assert!(
        got.is_none(),
        "no event expected after unsubscribe, got {got:?}"
    );
}

// ---------- AC3: fan-out ----------

#[test]
fn two_subscribers_both_receive_event() {
    let h = TestHarness::new(vec![]);
    let (mut r1, mut w1) = h.connect();
    let (mut r2, mut w2) = h.connect();

    write_msg(&mut w1, &ClientMessage::Subscribe).unwrap();
    write_msg(&mut w2, &ClientMessage::Subscribe).unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    while h.broadcaster.sub_count() < 2 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.broadcaster.sub_count(), 2);

    let msg = sample_event("fanout");
    h.broadcaster.publish(msg.clone());

    let got1 = read_with_timeout(&mut r1, Duration::from_secs(2)).expect("client1");
    let got2 = read_with_timeout(&mut r2, Duration::from_secs(2)).expect("client2");
    assert_eq!(got1, msg);
    assert_eq!(got2, msg);
}

// ---------- AC4: cancel ----------

#[test]
fn cancel_by_agent_id_signals_cancel_sender() {
    let h = TestHarness::new(vec![]);
    let (cancel_tx, cancel_rx) = unbounded::<()>();
    h.cancel_map.lock().unwrap().insert("a1".into(), cancel_tx);

    let (_reader, mut writer) = h.connect();
    write_msg(
        &mut writer,
        &ClientMessage::Cancel {
            agent_id: Some("a1".into()),
            session_id: None,
        },
    )
    .unwrap();

    cancel_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("cancel signal");
}

#[test]
fn cancel_by_session_id_signals_cancel_sender() {
    let h = TestHarness::new(vec![]);
    let (cancel_tx, cancel_rx) = unbounded::<()>();
    h.cancel_map.lock().unwrap().insert("s1".into(), cancel_tx);

    let (_reader, mut writer) = h.connect();
    write_msg(
        &mut writer,
        &ClientMessage::Cancel {
            agent_id: None,
            session_id: Some("s1".into()),
        },
    )
    .unwrap();

    cancel_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("cancel signal by session id");
}

#[test]
fn cancel_unknown_id_returns_error_message() {
    let h = TestHarness::new(vec![]);
    let (mut reader, mut writer) = h.connect();
    write_msg(
        &mut writer,
        &ClientMessage::Cancel {
            agent_id: Some("nope".into()),
            session_id: None,
        },
    )
    .unwrap();

    let got = read_with_timeout(&mut reader, Duration::from_secs(2)).expect("error response");
    match got {
        DaemonMessage::Error { message } => {
            assert!(message.contains("unknown"), "unexpected error: {message}");
        }
        other => panic!("expected Error, got {other:?}"),
    }

    // Conn stays open: follow-up Status must succeed.
    write_msg(&mut writer, &ClientMessage::Status).unwrap();
    let got2 = read_with_timeout(&mut reader, Duration::from_secs(2)).expect("status response");
    assert!(matches!(got2, DaemonMessage::DaemonStatus { .. }));
}

// ---------- AC5: status ----------

#[test]
fn status_returns_daemon_status_with_running_agents() {
    let h = TestHarness::new(vec![snap("a1"), snap("a2")]);
    let (mut reader, mut writer) = h.connect();

    write_msg(&mut writer, &ClientMessage::Status).unwrap();
    let got = read_with_timeout(&mut reader, Duration::from_secs(2)).expect("status response");
    match got {
        DaemonMessage::DaemonStatus { agents } => {
            assert_eq!(agents.len(), 2);
            assert_eq!(agents[0].agent_id, "a1");
            assert_eq!(agents[1].agent_id, "a2");
        }
        other => panic!("expected DaemonStatus, got {other:?}"),
    }
}

#[test]
fn status_with_no_agents_returns_empty_list() {
    let h = TestHarness::new(vec![]);
    let (mut reader, mut writer) = h.connect();
    write_msg(&mut writer, &ClientMessage::Status).unwrap();
    let got = read_with_timeout(&mut reader, Duration::from_secs(2)).expect("status response");
    match got {
        DaemonMessage::DaemonStatus { agents } => assert!(agents.is_empty()),
        other => panic!("expected DaemonStatus, got {other:?}"),
    }
}

// ---------- AC6: kick ----------

#[test]
fn kick_msg_sends_on_wake_channel() {
    let h = TestHarness::new(vec![]);
    let (_reader, mut writer) = h.connect();
    write_msg(&mut writer, &ClientMessage::Kick).unwrap();
    h.wake_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("wake signal");
}

// ---------- Error path ----------

#[test]
fn malformed_json_returns_error_keeps_conn_open() {
    use std::io::Write;
    let h = TestHarness::new(vec![]);
    let (mut reader, mut writer) = h.connect();

    writer.write_all(b"not json\n").unwrap();
    writer.flush().unwrap();

    let got = read_with_timeout(&mut reader, Duration::from_secs(2)).expect("error response");
    match got {
        DaemonMessage::Error { message } => {
            assert!(
                message.contains("malformed") || message.contains("expected"),
                "unexpected error: {message}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }

    // Follow up with a valid Status.
    write_msg(&mut writer, &ClientMessage::Status).unwrap();
    let got2 = read_with_timeout(&mut reader, Duration::from_secs(2)).expect("status response");
    assert!(matches!(got2, DaemonMessage::DaemonStatus { .. }));
}
