pub mod broadcaster;
pub mod client;
pub mod framing;
pub mod handler;
pub mod protocol;
pub mod state;

/// Relative path (from workspace root) to the daemon IPC socket.
///
/// Note: `engine::daemon::SOCKET_REL_PATH` is a duplicate constant with the
/// same value, kept private to that module. Both should converge on this
/// constant in a future cleanup.
pub const DAEMON_SOCKET_REL_PATH: &str = ".lazyspec/daemon.sock";
