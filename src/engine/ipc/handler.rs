//! Per-connection handler for the daemon's Unix socket. Spawns a reader thread
//! that frames `ClientMessage` JSON onto a crossbeam channel. The main handler
//! thread `select!`s between incoming client messages and (when subscribed)
//! broadcaster events, writing responses on the same socket.

use std::io::{BufReader, BufWriter};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use crossbeam_channel::{bounded, select, Receiver};

use crate::engine::ipc::framing::{read_msg, write_msg};
use crate::engine::ipc::protocol::{ClientMessage, DaemonMessage};
use crate::engine::ipc::state::DaemonState;

const CLIENT_QUEUE_CAP: usize = 64;

/// Handles a single accepted unix-socket connection. Blocks until the client
/// disconnects or an unrecoverable IO error occurs.
pub fn handle_connection(stream: UnixStream, state: Arc<DaemonState>) {
    if let Err(e) = run(stream, state) {
        eprintln!("warning: ipc handler error: {e}");
    }
}

fn run(stream: UnixStream, state: Arc<DaemonState>) -> Result<()> {
    let reader_stream = stream.try_clone()?;
    let mut writer = BufWriter::new(stream);

    let (client_tx, client_rx) = bounded::<Result<ClientMessage>>(CLIENT_QUEUE_CAP);

    let reader_handle = thread::spawn(move || {
        let mut r = BufReader::new(reader_stream);
        loop {
            match read_msg::<_, ClientMessage>(&mut r) {
                Ok(Some(msg)) => {
                    if client_tx.send(Ok(msg)).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    let _ = client_tx.send(Err(e));
                }
            }
        }
    });

    let mut sub_rx: Option<Receiver<DaemonMessage>> = None;

    loop {
        let event = if let Some(rx) = sub_rx.as_ref() {
            select! {
                recv(client_rx) -> incoming => Event::Client(incoming),
                recv(rx) -> ev => Event::Broadcast(ev),
            }
        } else {
            Event::Client(client_rx.recv())
        };

        match event {
            Event::Client(Err(_)) => break,
            Event::Client(Ok(Err(parse_err))) => {
                let err_msg = DaemonMessage::Error {
                    message: format!("malformed message: {parse_err}"),
                };
                if write_msg(&mut writer, &err_msg).is_err() {
                    break;
                }
            }
            Event::Client(Ok(Ok(msg))) => {
                if !dispatch(msg, &state, &mut sub_rx, &mut writer) {
                    break;
                }
            }
            Event::Broadcast(Err(_)) => {
                sub_rx = None;
            }
            Event::Broadcast(Ok(msg)) => {
                if write_msg(&mut writer, &msg).is_err() {
                    break;
                }
            }
        }
    }

    drop(writer);
    let _ = reader_handle.join();
    Ok(())
}

enum Event {
    Client(Result<Result<ClientMessage>, crossbeam_channel::RecvError>),
    Broadcast(Result<DaemonMessage, crossbeam_channel::RecvError>),
}

/// Returns false if the connection should be closed (write error).
fn dispatch(
    msg: ClientMessage,
    state: &DaemonState,
    sub_rx: &mut Option<Receiver<DaemonMessage>>,
    writer: &mut BufWriter<UnixStream>,
) -> bool {
    match msg {
        ClientMessage::Subscribe => {
            *sub_rx = Some(state.broadcaster.subscribe());
            true
        }
        ClientMessage::Unsubscribe => {
            *sub_rx = None;
            true
        }
        ClientMessage::Cancel {
            agent_id,
            session_id,
        } => {
            let target = {
                let map = state.cancel_map.lock().expect("cancel_map poisoned");
                let by_agent = agent_id.as_deref().and_then(|k| map.get(k).cloned());
                let by_session = session_id.as_deref().and_then(|k| map.get(k).cloned());
                by_agent.or(by_session)
            };
            match target {
                Some(tx) => {
                    let _ = tx.try_send(());
                    true
                }
                None => {
                    let err = DaemonMessage::Error {
                        message: "unknown agent or session id".into(),
                    };
                    write_msg(writer, &err).is_ok()
                }
            }
        }
        ClientMessage::Status => {
            let snap = state.snapshot_provider.snapshot();
            let resp = DaemonMessage::DaemonStatus { agents: snap };
            write_msg(writer, &resp).is_ok()
        }
        ClientMessage::Kick => {
            let _ = state.wake.try_send(());
            true
        }
    }
}
