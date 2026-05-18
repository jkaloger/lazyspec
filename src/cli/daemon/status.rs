//! `lazyspec daemon status` — single-shot snapshot query against a running daemon.
//!
//! Connects to the IPC socket, sends [`ClientMessage::Status`], reads one
//! [`DaemonMessage::DaemonStatus`] reply, renders it, and exits. No retry,
//! no spawn, no fork.
//!
//! ## JSON shape
//!
//! When `--json` is passed we serialize the `Vec<AgentSnapshot>` directly
//! (Choice A): the consumer wants the agents array, not the wire envelope
//! with its `type` tag.

use std::io::{self, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::engine::ipc::framing::{read_msg, write_msg};
use crate::engine::ipc::protocol::{AgentSnapshot, ClientMessage, DaemonMessage};
use crate::engine::ipc::DAEMON_SOCKET_REL_PATH;

#[derive(Debug)]
pub enum DaemonStatusError {
    NotRunning,
    Io(io::Error),
    Protocol(serde_json::Error),
}

impl std::fmt::Display for DaemonStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonStatusError::NotRunning => write!(f, "daemon not running"),
            DaemonStatusError::Io(e) => write!(f, "daemon status io error: {}", e),
            DaemonStatusError::Protocol(e) => write!(f, "daemon status protocol error: {}", e),
        }
    }
}

impl std::error::Error for DaemonStatusError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DaemonStatusError::NotRunning => None,
            DaemonStatusError::Io(e) => Some(e),
            DaemonStatusError::Protocol(e) => Some(e),
        }
    }
}

impl From<io::Error> for DaemonStatusError {
    fn from(e: io::Error) -> Self {
        DaemonStatusError::Io(e)
    }
}

impl From<serde_json::Error> for DaemonStatusError {
    fn from(e: serde_json::Error) -> Self {
        DaemonStatusError::Protocol(e)
    }
}

/// Connect to the daemon at `<root>/.lazyspec/daemon.sock`, request a status
/// snapshot, render it, and return. See module docs for JSON shape.
pub fn run(root: &Path, json: bool) -> Result<(), DaemonStatusError> {
    run_with_writer(root, json, &mut io::stdout())
}

/// Like [`run`], but renders into the supplied writer. The writer seam lets
/// tests capture the rendered payload without process spawning or stdout
/// redirection (DICTUM-004: I/O at trait seams). Nothing is written on error.
pub fn run_with_writer<W: Write>(
    root: &Path,
    json: bool,
    writer: &mut W,
) -> Result<(), DaemonStatusError> {
    let path = root.join(DAEMON_SOCKET_REL_PATH);

    let stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(e) if is_not_running(&e) => return Err(DaemonStatusError::NotRunning),
        Err(e) => return Err(DaemonStatusError::Io(e)),
    };

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut sock_writer = stream;

    write_msg(&mut sock_writer, &ClientMessage::Status).map_err(map_anyhow)?;

    let reply: Option<DaemonMessage> = read_msg(&mut reader).map_err(map_anyhow)?;

    let agents = match reply {
        Some(DaemonMessage::DaemonStatus { agents }) => agents,
        Some(_) => {
            return Err(DaemonStatusError::Io(io::Error::new(
                ErrorKind::InvalidData,
                "unexpected daemon reply",
            )));
        }
        None => {
            return Err(DaemonStatusError::Io(io::Error::new(
                ErrorKind::UnexpectedEof,
                "daemon closed connection without reply",
            )));
        }
    };

    render(&agents, json, writer)?;
    Ok(())
}

fn render<W: Write>(
    agents: &[AgentSnapshot],
    json: bool,
    w: &mut W,
) -> Result<(), DaemonStatusError> {
    if json {
        writeln!(w, "{}", serde_json::to_string(&agents)?)?;
        return Ok(());
    }

    let headers = [
        "AGENT_ID",
        "DOC_ID",
        "ELAPSED_MS",
        "TOKENS_IN",
        "TOKENS_OUT",
    ];
    let mut widths = headers.map(|h| h.len());

    for a in agents {
        widths[0] = widths[0].max(a.agent_id.len());
        widths[1] = widths[1].max(a.doc_id.len());
        widths[2] = widths[2].max(a.elapsed_ms.to_string().len());
        widths[3] = widths[3].max(a.tokens_in.to_string().len());
        widths[4] = widths[4].max(a.tokens_out.to_string().len());
    }

    writeln!(
        w,
        "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}",
        headers[0],
        headers[1],
        headers[2],
        headers[3],
        headers[4],
        w0 = widths[0],
        w1 = widths[1],
        w2 = widths[2],
        w3 = widths[3],
        w4 = widths[4],
    )?;

    for a in agents {
        writeln!(
            w,
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}",
            a.agent_id,
            a.doc_id,
            a.elapsed_ms,
            a.tokens_in,
            a.tokens_out,
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3],
            w4 = widths[4],
        )?;
    }

    Ok(())
}

/// True when a `connect` error means "no daemon listening here". Covers the
/// portable variants (`NotFound`, `ConnectionRefused`) plus `ENOTSOCK` raised
/// on macOS/Linux when the path exists but is a regular file (stale socket).
fn is_not_running(e: &io::Error) -> bool {
    if matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::NotFound) {
        return true;
    }
    // `io::Error::kind()` reports `Uncategorized` for ENOTSOCK; check raw.
    e.raw_os_error() == Some(libc::ENOTSOCK)
}

/// Collapse a framing-layer `anyhow::Error` into our typed error. Framing wraps
/// either an `io::Error` or a `serde_json::Error`; try those first, fall back
/// to a generic `Other` io error.
fn map_anyhow(e: anyhow::Error) -> DaemonStatusError {
    match e.downcast::<io::Error>() {
        Ok(io_err) => DaemonStatusError::Io(io_err),
        Err(e) => match e.downcast::<serde_json::Error>() {
            Ok(j) => DaemonStatusError::Protocol(j),
            Err(e) => DaemonStatusError::Io(io::Error::other(e.to_string())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_running_returned_for_missing_socket() {
        let td = tempfile::TempDir::new().unwrap();
        let err = run(td.path(), false).unwrap_err();
        assert!(
            matches!(err, DaemonStatusError::NotRunning),
            "expected NotRunning, got {:?}",
            err
        );
    }

    #[test]
    fn not_running_returned_for_stale_socket_file() {
        let td = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        std::fs::write(td.path().join(".lazyspec/daemon.sock"), b"stale").unwrap();
        let err = run(td.path(), false).unwrap_err();
        assert!(
            matches!(err, DaemonStatusError::NotRunning),
            "expected NotRunning, got {:?}",
            err
        );
    }
}
