use std::io::{self, BufReader, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use super::framing;
use super::protocol::{ClientMessage, DaemonMessage};

pub trait Connector: Send + 'static {
    type Stream: Read + Write + Send + 'static;
    fn connect(&self) -> io::Result<Self::Stream>;
}

pub struct SocketConnector {
    path: PathBuf,
}

impl SocketConnector {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Connector for SocketConnector {
    type Stream = UnixStream;
    fn connect(&self) -> io::Result<UnixStream> {
        UnixStream::connect(&self.path)
    }
}

pub trait Sleeper: Send + 'static {
    fn sleep(&self, d: Duration);
}

pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

pub struct BackoffSchedule {
    base_ms: u64,
    cap_ms: u64,
    attempt: u32,
}

impl BackoffSchedule {
    pub fn new(base_ms: u64, cap_ms: u64) -> Self {
        Self {
            base_ms,
            cap_ms,
            attempt: 0,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Duration {
        let shifted = self.base_ms.saturating_mul(1u64 << self.attempt.min(63));
        let ms = shifted.min(self.cap_ms);
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_millis(ms)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[allow(clippy::match_like_matches_macro)]
fn is_transient(e: &io::Error) -> bool {
    match e.kind() {
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::BrokenPipe
        | ErrorKind::UnexpectedEof
        | ErrorKind::NotFound
        | ErrorKind::ConnectionAborted
        | ErrorKind::TimedOut => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Reconnecting,
}

pub fn default_subscriber(
    socket_path: PathBuf,
) -> ReconnectingSubscriber<SocketConnector, ThreadSleeper> {
    ReconnectingSubscriber::new(
        SocketConnector::new(socket_path),
        ThreadSleeper,
        BackoffSchedule::new(250, 5000),
    )
}

pub struct ReconnectingSubscriber<C: Connector, S: Sleeper> {
    connector: C,
    sleeper: S,
    backoff: BackoffSchedule,
}

impl<C: Connector, S: Sleeper> ReconnectingSubscriber<C, S> {
    pub fn new(connector: C, sleeper: S, backoff: BackoffSchedule) -> Self {
        Self {
            connector,
            sleeper,
            backoff,
        }
    }

    pub fn events(self) -> Receiver<DaemonMessage> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let ReconnectingSubscriber {
            connector,
            sleeper,
            mut backoff,
        } = self;

        std::thread::spawn(move || {
            run_loop(connector, sleeper, &mut backoff, tx, None);
        });

        rx
    }

    pub fn events_with_state(self) -> (Receiver<DaemonMessage>, Receiver<ConnectionState>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (state_tx, state_rx) = crossbeam_channel::unbounded();
        let ReconnectingSubscriber {
            connector,
            sleeper,
            mut backoff,
        } = self;

        std::thread::spawn(move || {
            run_loop(connector, sleeper, &mut backoff, tx, Some(state_tx));
        });

        (rx, state_rx)
    }
}

fn run_loop<C: Connector, S: Sleeper>(
    connector: C,
    sleeper: S,
    backoff: &mut BackoffSchedule,
    tx: Sender<DaemonMessage>,
    state_tx: Option<Sender<ConnectionState>>,
) {
    let emit_state = |s: ConnectionState| {
        if let Some(tx) = state_tx.as_ref() {
            let _ = tx.send(s);
        }
    };

    loop {
        let stream = match connector.connect() {
            Ok(s) => s,
            Err(e) if is_transient(&e) => {
                emit_state(ConnectionState::Reconnecting);
                sleeper.sleep(backoff.next());
                continue;
            }
            Err(_) => return,
        };

        let (mut reader, mut writer) = match split_stream(stream) {
            Ok(pair) => pair,
            Err(_) => return,
        };

        if framing::write_msg(&mut writer, &ClientMessage::Subscribe).is_err() {
            emit_state(ConnectionState::Reconnecting);
            sleeper.sleep(backoff.next());
            continue;
        }

        emit_state(ConnectionState::Connected);

        loop {
            match framing::read_msg::<_, DaemonMessage>(&mut reader) {
                Ok(Some(msg)) => {
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    emit_state(ConnectionState::Reconnecting);
                    sleeper.sleep(backoff.next());
                    break;
                }
                Err(e) => {
                    let transient = e
                        .downcast_ref::<io::Error>()
                        .map(is_transient)
                        .unwrap_or(false);
                    if transient {
                        emit_state(ConnectionState::Reconnecting);
                        sleeper.sleep(backoff.next());
                        break;
                    } else {
                        return;
                    }
                }
            }
        }
    }
}

/// Split a duplex stream into a buffered reader and a writer, both backed by
/// the same underlying stream. Uses a shared `Arc<Mutex<...>>` so we don't
/// need `try_clone` (which is `UnixStream`-specific).
fn split_stream<T: Read + Write + Send + 'static>(
    stream: T,
) -> io::Result<(BufReader<ReadHalf<T>>, WriteHalf<T>)> {
    use std::sync::{Arc, Mutex};
    let shared = Arc::new(Mutex::new(stream));
    let r = ReadHalf {
        inner: Arc::clone(&shared),
    };
    let w = WriteHalf { inner: shared };
    Ok((BufReader::new(r), w))
}

struct ReadHalf<T> {
    inner: std::sync::Arc<std::sync::Mutex<T>>,
}

impl<T: Read> Read for ReadHalf<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut g = self.inner.lock().expect("poisoned");
        g.read(buf)
    }
}

struct WriteHalf<T> {
    inner: std::sync::Arc<std::sync::Mutex<T>>,
}

impl<T: Write> Write for WriteHalf<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut g = self.inner.lock().expect("poisoned");
        g.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut g = self.inner.lock().expect("poisoned");
        g.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::runner::AgentEvent;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    /// In-memory duplex pipe: writes go into `outgoing`, reads come from `incoming`.
    /// One end is given to the "subscriber" (this is `TestStream` returned by the
    /// mock connector). The "server" side reads from `outgoing` and writes into
    /// `incoming`.
    struct TestStream {
        incoming: Arc<Mutex<Vec<u8>>>, // server -> client (we read)
        outgoing: Arc<Mutex<Vec<u8>>>, // client -> server (we write)
        eof_signal: Arc<Mutex<bool>>,
    }

    impl Read for TestStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            loop {
                {
                    let mut g = self.incoming.lock().unwrap();
                    if !g.is_empty() {
                        let n = g.len().min(buf.len());
                        buf[..n].copy_from_slice(&g[..n]);
                        g.drain(..n);
                        return Ok(n);
                    }
                    if *self.eof_signal.lock().unwrap() {
                        return Ok(0);
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    impl Write for TestStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut g = self.outgoing.lock().unwrap();
            g.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Scripted connector. Each `connect()` call pops the next prepared
    /// outcome from the queue. When the queue is empty, returns
    /// `PermissionDenied` to keep the loop terminating predictably (or as
    /// requested by the script). Records all connect attempts.
    enum Outcome {
        Stream {
            incoming: Arc<Mutex<Vec<u8>>>,
            outgoing: Arc<Mutex<Vec<u8>>>,
            eof_signal: Arc<Mutex<bool>>,
        },
        Err(ErrorKind),
    }

    struct ScriptedConnector {
        script: Arc<Mutex<Vec<Outcome>>>,
        attempts: Arc<Mutex<u32>>,
        on_exhaust: ErrorKind,
    }

    impl Connector for ScriptedConnector {
        type Stream = TestStream;
        fn connect(&self) -> io::Result<TestStream> {
            *self.attempts.lock().unwrap() += 1;
            let mut s = self.script.lock().unwrap();
            if s.is_empty() {
                return Err(io::Error::from(self.on_exhaust));
            }
            match s.remove(0) {
                Outcome::Stream {
                    incoming,
                    outgoing,
                    eof_signal,
                } => Ok(TestStream {
                    incoming,
                    outgoing,
                    eof_signal,
                }),
                Outcome::Err(k) => Err(io::Error::from(k)),
            }
        }
    }

    struct RecordingSleeper {
        sleeps: Arc<Mutex<Vec<Duration>>>,
    }

    impl Sleeper for RecordingSleeper {
        fn sleep(&self, d: Duration) {
            self.sleeps.lock().unwrap().push(d);
        }
    }

    fn encode_event(agent_id: &str, delta: &str) -> Vec<u8> {
        let msg = DaemonMessage::AgentEvent {
            agent_id: agent_id.into(),
            session_id: "s1".into(),
            event: AgentEvent::Text {
                delta: delta.into(),
            },
        };
        let mut buf = Vec::new();
        framing::write_msg(&mut buf, &msg).unwrap();
        buf
    }

    #[test]
    fn is_transient_classifies() {
        use std::io::ErrorKind::*;
        let transient = [
            ConnectionRefused,
            ConnectionReset,
            BrokenPipe,
            UnexpectedEof,
            NotFound,
            ConnectionAborted,
            TimedOut,
        ];
        let permanent = [
            PermissionDenied,
            InvalidInput,
            InvalidData,
            AlreadyExists,
            AddrInUse,
            AddrNotAvailable,
            Unsupported,
            WouldBlock,
            Interrupted,
            Other,
        ];
        for k in transient {
            let e = io::Error::from(k);
            assert!(is_transient(&e), "expected transient for {k:?}");
        }
        for k in permanent {
            let e = io::Error::from(k);
            assert!(!is_transient(&e), "expected permanent for {k:?}");
        }
    }

    #[test]
    fn reconnect_resends_subscribe() {
        // First connection: one event then EOF. Second: another event then EOF.
        let in1 = Arc::new(Mutex::new(encode_event("a1", "first")));
        let out1 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof1 = Arc::new(Mutex::new(true));

        let in2 = Arc::new(Mutex::new(encode_event("a2", "second")));
        let out2 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof2 = Arc::new(Mutex::new(true));

        let script = vec![
            Outcome::Stream {
                incoming: Arc::clone(&in1),
                outgoing: Arc::clone(&out1),
                eof_signal: Arc::clone(&eof1),
            },
            Outcome::Stream {
                incoming: Arc::clone(&in2),
                outgoing: Arc::clone(&out2),
                eof_signal: Arc::clone(&eof2),
            },
        ];

        let connector = ScriptedConnector {
            script: Arc::new(Mutex::new(script)),
            attempts: Arc::new(Mutex::new(0)),
            on_exhaust: ErrorKind::PermissionDenied, // terminate after two streams
        };
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let sleeper = RecordingSleeper {
            sleeps: Arc::clone(&sleeps),
        };

        let sub = ReconnectingSubscriber::new(connector, sleeper, BackoffSchedule::new(10, 50));
        let rx = sub.events();

        let first = rx.recv_timeout(Duration::from_secs(2)).expect("first event");
        let second = rx.recv_timeout(Duration::from_secs(2)).expect("second event");

        match first {
            DaemonMessage::AgentEvent { agent_id, .. } => assert_eq!(agent_id, "a1"),
            other => panic!("unexpected: {other:?}"),
        }
        match second {
            DaemonMessage::AgentEvent { agent_id, .. } => assert_eq!(agent_id, "a2"),
            other => panic!("unexpected: {other:?}"),
        }

        // After two streams, on_exhaust=PermissionDenied -> permanent -> thread exits, channel closes.
        let _ = rx.recv_timeout(Duration::from_secs(2));

        let written1 = out1.lock().unwrap().clone();
        let written2 = out2.lock().unwrap().clone();
        assert!(
            !written1.is_empty() && written1.ends_with(b"\n"),
            "first connection should have written framed subscribe"
        );
        assert!(
            !written2.is_empty() && written2.ends_with(b"\n"),
            "second connection should have written framed subscribe"
        );
        let parsed1: ClientMessage =
            serde_json::from_slice(&written1[..written1.len() - 1]).unwrap();
        let parsed2: ClientMessage =
            serde_json::from_slice(&written2[..written2.len() - 1]).unwrap();
        assert_eq!(parsed1, ClientMessage::Subscribe);
        assert_eq!(parsed2, ClientMessage::Subscribe);
    }

    #[test]
    fn permanent_error_propagates() {
        let connector = ScriptedConnector {
            script: Arc::new(Mutex::new(vec![Outcome::Err(ErrorKind::PermissionDenied)])),
            attempts: Arc::new(Mutex::new(0)),
            on_exhaust: ErrorKind::PermissionDenied,
        };
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let sleeper = RecordingSleeper {
            sleeps: Arc::clone(&sleeps),
        };

        let sub = ReconnectingSubscriber::new(connector, sleeper, BackoffSchedule::new(10, 50));
        let rx = sub.events();

        let res = rx.recv_timeout(Duration::from_secs(2));
        assert!(res.is_err(), "channel should close on permanent error");

        assert_eq!(sleeps.lock().unwrap().len(), 0, "no backoff on permanent");
    }

    #[test]
    fn backoff_schedule_grows_exponentially_and_caps() {
        let mut b = BackoffSchedule::new(100, 400);
        assert_eq!(b.next(), Duration::from_millis(100));
        assert_eq!(b.next(), Duration::from_millis(200));
        assert_eq!(b.next(), Duration::from_millis(400));
        assert_eq!(b.next(), Duration::from_millis(400));
        assert_eq!(b.next(), Duration::from_millis(400));
    }

    #[test]
    fn receiver_drop_exits_helper_thread() {
        // Endless stream of events. Once receiver is dropped, send returns
        // SendError and thread exits.
        let incoming = Arc::new(Mutex::new(Vec::<u8>::new()));
        let outgoing = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof = Arc::new(Mutex::new(false));

        // Pre-load many events; refill via a feeder thread to keep it endless.
        for _ in 0..10 {
            let bytes = encode_event("a", "x");
            incoming.lock().unwrap().extend_from_slice(&bytes);
        }
        let feeder_incoming = Arc::clone(&incoming);
        let feeder_stop = Arc::new(Mutex::new(false));
        let feeder_stop_handle = Arc::clone(&feeder_stop);
        let feeder = std::thread::spawn(move || {
            while !*feeder_stop_handle.lock().unwrap() {
                {
                    let mut g = feeder_incoming.lock().unwrap();
                    if g.len() < 1024 {
                        let bytes = encode_event("a", "x");
                        g.extend_from_slice(&bytes);
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });

        let connector = ScriptedConnector {
            script: Arc::new(Mutex::new(vec![Outcome::Stream {
                incoming: Arc::clone(&incoming),
                outgoing: Arc::clone(&outgoing),
                eof_signal: Arc::clone(&eof),
            }])),
            attempts: Arc::new(Mutex::new(0)),
            on_exhaust: ErrorKind::PermissionDenied,
        };
        let sleeper = RecordingSleeper {
            sleeps: Arc::new(Mutex::new(Vec::new())),
        };

        let sub = ReconnectingSubscriber::new(connector, sleeper, BackoffSchedule::new(10, 50));
        let rx = sub.events();

        // Consume one event to confirm flowing, then drop.
        let _ = rx.recv_timeout(Duration::from_secs(2)).expect("event");
        drop(rx);

        // Helper thread should exit on next send. Wait until incoming buffer
        // stops being drained (signal that reader thread stopped) OR cap wait.
        let start = Instant::now();
        let mut last_len = incoming.lock().unwrap().len();
        let mut stable_since: Option<Instant> = None;
        loop {
            if start.elapsed() > Duration::from_secs(2) {
                *feeder_stop.lock().unwrap() = true;
                feeder.join().ok();
                panic!("helper thread did not exit after receiver drop");
            }
            std::thread::sleep(Duration::from_millis(20));
            let now_len = incoming.lock().unwrap().len();
            // Feeder keeps refilling; thread stopped if buffer grows to feeder
            // cap (~1024) without being drained.
            if now_len >= 1024 {
                if last_len == now_len {
                    let s = stable_since.get_or_insert(Instant::now());
                    if s.elapsed() > Duration::from_millis(100) {
                        break;
                    }
                } else {
                    stable_since = None;
                }
            }
            last_len = now_len;
        }

        *feeder_stop.lock().unwrap() = true;
        feeder.join().ok();
    }

    #[test]
    fn state_channel_emits_connected_then_reconnecting_on_eof() {
        // First connection: one event then EOF -> Connected then Reconnecting.
        // Second connect attempt exhausts script with PermissionDenied -> terminates.
        let incoming = Arc::new(Mutex::new(encode_event("a1", "hi")));
        let outgoing = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof = Arc::new(Mutex::new(true));

        let connector = ScriptedConnector {
            script: Arc::new(Mutex::new(vec![Outcome::Stream {
                incoming: Arc::clone(&incoming),
                outgoing: Arc::clone(&outgoing),
                eof_signal: Arc::clone(&eof),
            }])),
            attempts: Arc::new(Mutex::new(0)),
            on_exhaust: ErrorKind::PermissionDenied,
        };
        let sleeper = RecordingSleeper {
            sleeps: Arc::new(Mutex::new(Vec::new())),
        };

        let sub = ReconnectingSubscriber::new(connector, sleeper, BackoffSchedule::new(10, 50));
        let (rx, state_rx) = sub.events_with_state();

        let _ = rx.recv_timeout(Duration::from_secs(2)).expect("event");

        let first = state_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first state");
        assert_eq!(first, ConnectionState::Connected);

        let second = state_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second state");
        assert_eq!(second, ConnectionState::Reconnecting);
    }

    #[test]
    fn subscriber_resumes_streaming_after_transient_drop() {
        // First stream: emits "first" then EOF -> Reconnecting -> backoff
        // Second stream: emits "second" then EOF
        // Third connect attempt exhausts -> PermissionDenied -> permanent exit.
        let in1 = Arc::new(Mutex::new(encode_event("a1", "first")));
        let out1 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof1 = Arc::new(Mutex::new(true));

        let in2 = Arc::new(Mutex::new(encode_event("a2", "second")));
        let out2 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof2 = Arc::new(Mutex::new(true));

        let connector = ScriptedConnector {
            script: Arc::new(Mutex::new(vec![
                Outcome::Stream {
                    incoming: Arc::clone(&in1),
                    outgoing: Arc::clone(&out1),
                    eof_signal: Arc::clone(&eof1),
                },
                Outcome::Stream {
                    incoming: Arc::clone(&in2),
                    outgoing: Arc::clone(&out2),
                    eof_signal: Arc::clone(&eof2),
                },
            ])),
            attempts: Arc::new(Mutex::new(0)),
            on_exhaust: ErrorKind::PermissionDenied,
        };
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let sleeper = RecordingSleeper {
            sleeps: Arc::clone(&sleeps),
        };

        let sub = ReconnectingSubscriber::new(connector, sleeper, BackoffSchedule::new(10, 50));
        let (rx, state_rx) = sub.events_with_state();

        let first = rx.recv_timeout(Duration::from_secs(2)).expect("first event");
        match first {
            DaemonMessage::AgentEvent { event, .. } => match event {
                AgentEvent::Text { delta } => assert_eq!(delta, "first"),
                other => panic!("unexpected event: {other:?}"),
            },
            other => panic!("unexpected message: {other:?}"),
        }

        let second = rx.recv_timeout(Duration::from_secs(2)).expect("second event");
        match second {
            DaemonMessage::AgentEvent { event, .. } => match event {
                AgentEvent::Text { delta } => assert_eq!(delta, "second"),
                other => panic!("unexpected event: {other:?}"),
            },
            other => panic!("unexpected message: {other:?}"),
        }

        // Event channel closes cleanly after permanent error.
        let closed = rx.recv_timeout(Duration::from_secs(2));
        assert!(closed.is_err(), "event channel should close cleanly");

        // Collect state transitions. The thread has exited (event channel
        // closed) so the state sender is dropped; draining yields all states.
        let mut states = Vec::new();
        while let Ok(s) = state_rx.recv_timeout(Duration::from_millis(200)) {
            states.push(s);
        }
        assert_eq!(
            states,
            vec![
                ConnectionState::Connected,
                ConnectionState::Reconnecting,
                ConnectionState::Connected,
                ConnectionState::Reconnecting,
            ],
            "expected Connected -> Reconnecting -> Connected -> Reconnecting"
        );
    }

    #[test]
    fn state_channel_drop_does_not_break_event_stream() {
        let in1 = Arc::new(Mutex::new(encode_event("a1", "first")));
        let out1 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof1 = Arc::new(Mutex::new(true));

        let in2 = Arc::new(Mutex::new(encode_event("a2", "second")));
        let out2 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let eof2 = Arc::new(Mutex::new(true));

        let connector = ScriptedConnector {
            script: Arc::new(Mutex::new(vec![
                Outcome::Stream {
                    incoming: Arc::clone(&in1),
                    outgoing: Arc::clone(&out1),
                    eof_signal: Arc::clone(&eof1),
                },
                Outcome::Stream {
                    incoming: Arc::clone(&in2),
                    outgoing: Arc::clone(&out2),
                    eof_signal: Arc::clone(&eof2),
                },
            ])),
            attempts: Arc::new(Mutex::new(0)),
            on_exhaust: ErrorKind::PermissionDenied,
        };
        let sleeper = RecordingSleeper {
            sleeps: Arc::new(Mutex::new(Vec::new())),
        };

        let sub = ReconnectingSubscriber::new(connector, sleeper, BackoffSchedule::new(10, 50));
        let (rx, state_rx) = sub.events_with_state();

        drop(state_rx);

        let first = rx.recv_timeout(Duration::from_secs(2)).expect("first");
        let second = rx.recv_timeout(Duration::from_secs(2)).expect("second");
        match first {
            DaemonMessage::AgentEvent { agent_id, .. } => assert_eq!(agent_id, "a1"),
            other => panic!("unexpected: {other:?}"),
        }
        match second {
            DaemonMessage::AgentEvent { agent_id, .. } => assert_eq!(agent_id, "a2"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
