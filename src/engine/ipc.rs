pub mod broadcaster;
pub mod client;
pub mod framing;
pub mod handler;
pub mod protocol;
pub mod state;

use std::path::PathBuf;

pub use client::{default_subscriber, ConnectionState, ReconnectingSubscriber};

/// Relative path (from workspace root) to the daemon IPC socket.
///
/// Note: `engine::daemon::SOCKET_REL_PATH` is a duplicate constant with the
/// same value, kept private to that module. Both should converge on this
/// constant in a future cleanup.
pub const DAEMON_SOCKET_REL_PATH: &str = ".lazyspec/daemon.sock";

/// One-shot send of a `ClientMessage` over a fresh connection.
///
/// Returns `Err` on connection or write failure -- callers decide whether to
/// surface the error or absorb it (e.g. when the daemon is offline, callers
/// may treat this as best-effort).
pub fn send_one_shot<C: client::Connector>(
    connector: &C,
    msg: &protocol::ClientMessage,
) -> std::io::Result<()> {
    let mut stream = connector.connect()?;
    framing::write_msg(&mut stream, msg).map_err(std::io::Error::other)
}

/// Send a single `Kick` message to the daemon at `socket_path`. Convenience
/// wrapper around `send_one_shot` for TUI/CLI callers.
pub fn send_kick(socket_path: PathBuf) -> std::io::Result<()> {
    let connector = client::SocketConnector::new(socket_path);
    send_one_shot(&connector, &protocol::ClientMessage::Kick)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind, Read, Write};
    use std::sync::{Arc, Mutex};

    struct CapturedStream {
        outgoing: Arc<Mutex<Vec<u8>>>,
    }

    impl Read for CapturedStream {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    impl Write for CapturedStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.outgoing.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct ScriptedConnector {
        outgoing: Arc<Mutex<Vec<u8>>>,
        fail_with: Option<ErrorKind>,
    }

    impl client::Connector for ScriptedConnector {
        type Stream = CapturedStream;
        fn connect(&self) -> io::Result<CapturedStream> {
            if let Some(k) = self.fail_with {
                return Err(io::Error::from(k));
            }
            Ok(CapturedStream {
                outgoing: Arc::clone(&self.outgoing),
            })
        }
    }

    #[test]
    fn send_one_shot_writes_kick_message() {
        let outgoing = Arc::new(Mutex::new(Vec::<u8>::new()));
        let connector = ScriptedConnector {
            outgoing: Arc::clone(&outgoing),
            fail_with: None,
        };

        send_one_shot(&connector, &protocol::ClientMessage::Kick).expect("send ok");

        let written = outgoing.lock().unwrap().clone();
        assert!(!written.is_empty(), "should have written framed kick");
        assert!(written.ends_with(b"\n"), "should be newline-framed");
        let parsed: protocol::ClientMessage =
            serde_json::from_slice(&written[..written.len() - 1]).unwrap();
        assert_eq!(parsed, protocol::ClientMessage::Kick);
    }

    #[test]
    fn send_one_shot_returns_err_on_connect_failure() {
        let outgoing = Arc::new(Mutex::new(Vec::<u8>::new()));
        let connector = ScriptedConnector {
            outgoing,
            fail_with: Some(ErrorKind::ConnectionRefused),
        };

        let res = send_one_shot(&connector, &protocol::ClientMessage::Kick);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), ErrorKind::ConnectionRefused);
    }
}
