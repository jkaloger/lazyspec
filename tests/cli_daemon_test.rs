//! Integration tests for `lazyspec daemon` — STORY-121 ACs 1-8.
//!
//! Each test spawns the daemon as a real subprocess in an isolated `TempDir`,
//! sends real POSIX signals via `/bin/kill`, and asserts observable behavior.
//! No new crate deps: signals are delivered by shelling out, mirroring the
//! host_id primitive's convention.

mod common;

use common::TestFixture;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SOCK_REL: &str = ".lazyspec/daemon.sock";
const SPAWN_TIMEOUT: Duration = Duration::from_secs(2);
const EXIT_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// RAII guard so a panicking test still terminates its daemon subprocess.
struct DaemonHandle {
    child: Option<Child>,
}

impl DaemonHandle {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn pid(&self) -> u32 {
        self.child.as_ref().expect("child taken").id()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.as_mut().expect("child taken").try_wait()
    }

    fn wait_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.try_wait().expect("try_wait") {
                Some(status) => return Some(status),
                None => std::thread::sleep(POLL_INTERVAL),
            }
        }
        None
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Only SIGKILL if still running — avoids "No such process" noise
            // when a test killed the daemon and waited for exit already.
            if matches!(child.try_wait(), Ok(None)) {
                let _ = Command::new("kill")
                    .arg("-KILL")
                    .arg(child.id().to_string())
                    .stderr(Stdio::null())
                    .status();
                let _ = child.wait();
            }
        }
    }
}

/// Send `SIG<sig>` to `pid` via `/bin/kill`. `sig` is `"TERM"`, `"INT"`, etc.
fn kill_signal(pid: u32, sig: &str) {
    let status = Command::new("kill")
        .arg(format!("-{}", sig))
        .arg(pid.to_string())
        .status()
        .expect("kill exec");
    assert!(status.success(), "kill -{sig} {pid} failed");
}

/// Build a workspace fixture with the minimum config required to run the daemon:
/// `[coordination]` (required by `Daemon::new`) plus the standard doc tree.
fn daemon_workspace() -> TestFixture {
    let fixture = TestFixture::new();
    std::fs::write(
        fixture.root().join(".lazyspec.toml"),
        "[naming]\npattern = \"{type}-{n:03}-{title}.md\"\n\n[coordination]\n",
    )
    .unwrap();
    fixture
}

/// Spawn `lazyspec daemon` against `workspace` and wait up to `SPAWN_TIMEOUT`
/// for the socket file to appear AND accept a connection. Stdout/stderr are
/// piped so they don't pollute test output. On timeout we kill the child and
/// panic.
fn spawn_daemon(workspace: &Path) -> DaemonHandle {
    let child = Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .arg("daemon")
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lazyspec daemon");

    let mut handle = DaemonHandle::new(child);
    let sock_path = workspace.join(SOCK_REL);
    let deadline = Instant::now() + SPAWN_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(Some(status)) = handle.try_wait() {
            panic!("daemon exited before binding socket: {:?}", status);
        }
        if sock_path.exists() && UnixStream::connect(&sock_path).is_ok() {
            return handle;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    panic!(
        "daemon socket {} not ready within {:?}",
        sock_path.display(),
        SPAWN_TIMEOUT
    );
}

fn sock_path(workspace: &Path) -> PathBuf {
    workspace.join(SOCK_REL)
}

// ─── AC1 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_blocks_until_signal() {
    let fixture = daemon_workspace();
    let mut handle = spawn_daemon(fixture.root());

    // Sock is up; daemon must still be running.
    assert!(
        handle.try_wait().expect("try_wait").is_none(),
        "daemon should still be blocking in foreground"
    );

    // Brief observation window — it must remain alive without input.
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        handle.try_wait().expect("try_wait").is_none(),
        "daemon exited prematurely"
    );

    kill_signal(handle.pid(), "TERM");
    let status = handle
        .wait_exit(EXIT_TIMEOUT)
        .expect("daemon did not exit after SIGTERM");
    assert!(status.success(), "daemon exit status: {:?}", status);
}

// ─── AC2 + AC4 ────────────────────────────────────────────────────────────

#[test]
fn daemon_sigterm_releases_leases_and_unlinks_sock() {
    let fixture = daemon_workspace();
    let mut handle = spawn_daemon(fixture.root());
    let sock = sock_path(fixture.root());
    assert!(sock.exists(), "precondition: sock present after startup");

    kill_signal(handle.pid(), "TERM");
    let status = handle
        .wait_exit(EXIT_TIMEOUT)
        .expect("daemon did not exit after SIGTERM");
    assert!(status.success(), "expected exit 0, got: {:?}", status);

    // AC2: socket unlinked on graceful shutdown.
    // (Deeper lease-release semantics are covered by engine unit tests in T2/T3.)
    assert!(
        !sock.exists(),
        "socket {} should be unlinked after SIGTERM",
        sock.display()
    );
}

// ─── AC3 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_sigint_same_path_as_sigterm() {
    let fixture = daemon_workspace();
    let mut handle = spawn_daemon(fixture.root());
    let sock = sock_path(fixture.root());

    kill_signal(handle.pid(), "INT");
    let status = handle
        .wait_exit(EXIT_TIMEOUT)
        .expect("daemon did not exit after SIGINT");
    assert!(status.success(), "expected exit 0, got: {:?}", status);
    assert!(!sock.exists(), "socket should be unlinked after SIGINT");
}

// ─── AC4 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_binds_and_listens() {
    let fixture = daemon_workspace();
    let mut handle = spawn_daemon(fixture.root());
    let sock = sock_path(fixture.root());

    // spawn_daemon already proved one connect; do an independent connect to
    // assert the listener is accepting, not just that the inode exists.
    let stream = UnixStream::connect(&sock).expect("connect to daemon socket");
    drop(stream);

    kill_signal(handle.pid(), "TERM");
    handle.wait_exit(EXIT_TIMEOUT).expect("daemon exit");
}

// ─── AC5 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_replaces_stale_socket() {
    let fixture = daemon_workspace();
    let sock = sock_path(fixture.root());
    std::fs::create_dir_all(sock.parent().unwrap()).unwrap();
    std::fs::write(&sock, b"not a socket").unwrap();
    assert!(sock.exists(), "precondition: stale file at sock path");

    // spawn_daemon returns once the socket is live — proves the stale file
    // was replaced.
    let mut handle = spawn_daemon(fixture.root());
    let stream = UnixStream::connect(&sock).expect("connect to replaced socket");
    drop(stream);

    kill_signal(handle.pid(), "TERM");
    handle.wait_exit(EXIT_TIMEOUT).expect("daemon exit");
}

// ─── AC6 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_refuses_second_instance() {
    let fixture = daemon_workspace();
    let mut handle_a = spawn_daemon(fixture.root());
    let sock = sock_path(fixture.root());

    // Spawn B against the same workspace. It must exit non-zero promptly.
    let child_b = Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .arg("daemon")
        .current_dir(fixture.root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn second daemon");
    let mut handle_b = DaemonHandle::new(child_b);
    let status_b = handle_b
        .wait_exit(Duration::from_secs(2))
        .expect("second daemon should refuse and exit promptly");
    assert!(
        !status_b.success(),
        "second daemon should fail; status={:?}",
        status_b
    );

    // A must still be alive and answering.
    assert!(
        handle_a.try_wait().expect("try_wait").is_none(),
        "daemon A should still be running after B's refusal"
    );
    UnixStream::connect(&sock).expect("daemon A should still be accepting");

    kill_signal(handle_a.pid(), "TERM");
    handle_a.wait_exit(EXIT_TIMEOUT).expect("daemon A exit");
}

// ─── AC7 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_does_not_fork_or_pidfile() {
    let fixture = daemon_workspace();
    let mut handle = spawn_daemon(fixture.root());
    let child_pid = handle.pid();

    // No PID file written into the workspace.
    assert!(
        !fixture.root().join(".lazyspec/daemon.pid").exists(),
        "daemon must not write a PID file"
    );

    // Parent of the spawned daemon must be THIS test process — i.e. the
    // daemon did not fork+detach to init/launchd.
    let ppid_out = Command::new("ps")
        .args(["-o", "ppid=", "-p", &child_pid.to_string()])
        .output()
        .expect("ps -o ppid=");
    let ppid_str = String::from_utf8_lossy(&ppid_out.stdout).trim().to_string();
    let ppid: u32 = ppid_str
        .parse()
        .unwrap_or_else(|e| panic!("parse ppid {:?}: {}", ppid_str, e));
    assert_eq!(
        ppid,
        std::process::id(),
        "daemon parent should be the test process (no fork/detach)"
    );

    kill_signal(handle.pid(), "TERM");
    handle.wait_exit(EXIT_TIMEOUT).expect("daemon exit");
}

// ─── AC8 ──────────────────────────────────────────────────────────────────

#[test]
fn daemon_deployment_samples_present_in_readme() {
    let readme_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let readme = std::fs::read_to_string(&readme_path)
        .unwrap_or_else(|e| panic!("read {}: {}", readme_path.display(), e));

    // systemd sample markers
    assert!(readme.contains("[Unit]"), "README missing systemd [Unit]");
    assert!(
        readme.contains("ExecStart="),
        "README missing systemd ExecStart="
    );
    assert!(
        readme.contains("lazyspec daemon"),
        "README missing `lazyspec daemon` invocation"
    );

    // launchd plist markers
    assert!(
        readme.contains("<key>ProgramArguments</key>"),
        "README missing launchd ProgramArguments key"
    );
    assert!(
        readme.contains("<key>Label</key>"),
        "README missing launchd Label key"
    );
    assert!(
        readme.contains("au.com.inlight.lazyspec"),
        "README missing launchd bundle id au.com.inlight.lazyspec"
    );
}
