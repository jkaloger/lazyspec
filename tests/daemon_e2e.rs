//! End-to-end smoke test against a real [`Daemon`]. Drives the IPC layer through
//! the same `accept_loop` and `DaemonState` wiring that production uses, without
//! the heavyweight orchestration / agent spawn machinery — that's covered in
//! `tests/ipc.rs` at the handler level.

use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::bounded;
use tempfile::TempDir;

use lazyspec::engine::daemon::{Daemon, LeaseReleaser};
use lazyspec::engine::ipc::framing::{read_msg, write_msg};
use lazyspec::engine::ipc::protocol::{ClientMessage, DaemonMessage};

const SOCKET_REL_PATH: &str = ".lazyspec/daemon.sock";

struct NoopReleaser;

impl LeaseReleaser for NoopReleaser {
    fn release_host_leases(&self, _host_prefix: &str) -> Result<()> {
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

#[test]
fn daemon_e2e_status_response_works() {
    let td = TempDir::new().unwrap();
    std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
    let sock_path = td.path().join(SOCKET_REL_PATH);

    let mut daemon = Daemon::with_lease_releaser(
        td.path().to_path_buf(),
        sock_path.clone(),
        "host-e2e:0001".to_string(),
        Box::new(NoopReleaser),
    );

    let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
    let handle = thread::spawn(move || daemon.run(shutdown_rx));

    assert!(
        wait_for_socket(&sock_path, Duration::from_secs(2)),
        "daemon socket never became available"
    );

    let stream = UnixStream::connect(&sock_path).expect("connect to daemon");
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);

    write_msg(&mut writer, &ClientMessage::Status).expect("write status");

    reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let resp: Option<DaemonMessage> = read_msg(&mut reader).expect("read response");
    match resp {
        Some(DaemonMessage::DaemonStatus { agents }) => assert!(agents.is_empty()),
        other => panic!("expected DaemonStatus, got {other:?}"),
    }

    shutdown_tx.send(()).unwrap();
    let result = handle.join().expect("daemon thread panicked");
    assert!(result.is_ok(), "daemon returned error: {:?}", result);
}
