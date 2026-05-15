---
title: IPC protocol and daemon handler
type: iteration
status: accepted
author: agent
date: 2026-05-15
tags: []
related:
- implements: STORY-123
---

## Scope

Iter A of STORY-123. AC1-6. Daemon-side IPC: typed msg protocol, per-conn handler replacing `accept_loop` stub, fan-out broadcaster, cancel routing through tick `RunningAgent`, status snapshot from tick state, kick wake on poll sleep.

## In Scope

- Typed msg enums (`ClientMessage`, `DaemonMessage`) w/ `#[serde(tag = "type")]`, snake_case variants matching RFC-041 wire names.
- Newline-delim JSON framing — read/write helpers, one msg per `\n`, no embedded newlines.
- Per-conn handler. Replaces stub at `accept_loop` in `src/engine/daemon.rs:424`. Spawns thread per accepted conn. Parses one msg at a time, dispatches.
- `Broadcaster`: clonable sender handle. Tick loop pushes `agent_event` / `agent_status`; handler `subscribe` registers a per-conn sink, `unsubscribe` drops it. Fan-out to N subs.
- Cancel dispatch map: lookup by both `agent_id` and `session_id`. Routes to `RunningAgent.cancel: Sender<()>` in `tick.rs`. Shared via `Arc<Mutex<...>>` exposed through trait seam.
- Status snapshot: handler asks tick state for `Vec<AgentSnapshot>` (id, doc_id, elapsed, tokens_in, tokens_out). Returns single `daemon_status` msg.
- Kick wake: add wake `Sender<()>` to `TickLoop`. Replace blocking `clock.sleep` w/ select on shutdown + wake + sleep. `kick` msg sends one on wake channel; next tick fires immediately.
- Error msg: malformed JSON / unknown type / missing field emits `{"type":"error","message":"..."}`, conn stays open.

New module: `src/engine/ipc/` w/ `mod.rs`, `protocol.rs`, `framing.rs`, `handler.rs`, `broadcaster.rs`, `state.rs`.

## Out of Scope

- AC7, AC8: `lazyspec daemon status [--json]` CLI subcommand — Iter B.
- AC9: subscriber reconnect helper — Iter B.
- TUI consumer of stream — slice 7.
- `agent_event` / `agent_status` payload schemas beyond v1 minimum — slice 3 already shipped `AgentEvent` enum; this iter serialises existing variants only.
- Status mutation by daemon — still forbidden.
- Windows named-pipe transport — deferred beyond v1.
- CLI changes anywhere. Engine-only iter (dictum 3).

## Test Plan

Integration tests in `tests/ipc.rs`. Real `UnixListener` + `UnixStream` via `TempDir`. Bounded crossbeam channels w/ explicit `recv_timeout(Duration::from_secs(2))` instead of sleeps. Real types, no mocks at framing layer.

Unit tests for protocol enums + framing live alongside in `src/engine/ipc/*`.

### Unit tests (in `src/engine/ipc/`)

**AC1 framing**
- `framing::tests::write_msg_appends_newline` — write one msg, buf ends w/ `\n`, no embedded.
- `framing::tests::read_msg_returns_one_per_line` — feed two concatenated msgs, get two reads.
- `framing::tests::read_msg_eof_returns_none` — closed stream → `Ok(None)`.
- `framing::tests::read_msg_malformed_json_returns_err` — non-JSON line → parse err propagates.
- `protocol::tests::client_msg_round_trips_subscribe_unsubscribe_cancel_status_kick` — every variant survives serde.
- `protocol::tests::daemon_msg_round_trips_event_status_daemon_status_error`.
- `protocol::tests::cancel_accepts_agent_id_or_session_id` — both shapes deserialise.
- `protocol::tests::unknown_type_returns_serde_err`.

**AC3 broadcaster**
- `broadcaster::tests::publish_with_no_subs_is_noop`.
- `broadcaster::tests::two_subs_each_receive_published_event` — register 2 subs, publish 1 event, both rx recv same.
- `broadcaster::tests::dropped_sub_does_not_block_publish` — sub rx dropped, publish still ok, remaining subs receive.
- `broadcaster::tests::unsubscribe_drops_sink` — explicit unsub: subsequent publish not delivered.

### Integration tests (in `tests/ipc.rs`)

Helper `spawn_test_daemon(temp: &TempDir) -> (DaemonHandle, PathBuf)` binds socket at `temp.path().join(".lazyspec/daemon.sock")`, wires fake `TickState` + `Broadcaster`, returns handle + sock path. Shutdown via `Drop`.

**AC1 newline framing on wire**
- `ipc::framing_over_socket_is_newline_json` — connect, write `{"type":"status"}\n`, read response, assert response ends in `\n`, payload parses as JSON.

**AC2 subscribe receives streamed events**
- `ipc::subscribed_client_receives_published_event` — connect, send `subscribe`, publish `agent_event` via broadcaster, recv on stream w/ 2s timeout.
- `ipc::unsubscribe_stops_delivery` — sub then unsub; publish; assert no recv within 200ms bounded read.
- `ipc::dropped_connection_drops_subscription` — connect, sub, drop stream; publish; no panic, broadcaster state cleaned (assert via `broadcaster.sub_count() == 0`).

**AC3 fan-out**
- `ipc::two_subscribers_both_receive_event` — two conns subscribe, single publish, both recv.

**AC4 cancel routes to RunningAgent**
- `ipc::cancel_by_agent_id_signals_cancel_sender` — fake tick state registers agent w/ known id + a recording `Sender<()>`; send `{"type":"cancel","agent_id":"a1"}`; recv on cancel rx w/ 2s timeout.
- `ipc::cancel_by_session_id_signals_cancel_sender` — same but `session_id` lookup.
- `ipc::cancel_unknown_id_returns_error_message` — id not in dispatch map → `error` msg, conn stays open (write subsequent `status` after, succeeds).

**AC5 status snapshot**
- `ipc::status_returns_daemon_status_with_running_agents` — seed tick state w/ 2 fake agents; send `status`; parse response as `daemon_status`; assert 2 entries w/ expected ids, doc_ids, tokens.
- `ipc::status_with_no_agents_returns_empty_list`.

**AC6 kick wakes tick**
- `tick::kick_wake_interrupts_poll_sleep` (in `tick.rs` tests) — fake clock blocks on `sleep`; send wake; `run_once` returns before sleep duration elapses. Use `clock` w/ instrumented `sleep` that selects on wake rx.
- `ipc::kick_msg_sends_on_wake_channel` — handler-level test: send `kick` over socket, recv `()` on wake rx w/ 2s timeout.

**Error path**
- `ipc::malformed_json_returns_error_keeps_conn_open` — write `not json\n`, recv error msg, follow up `status`, recv ok response.

All recv assertions use `recv_timeout(Duration::from_secs(2))` so test failures show "timeout" not "hang".

## Changes

### 1. Define wire protocol enums
**ACs**: AC1
**Files**: new `src/engine/ipc/protocol.rs`, new `src/engine/ipc/mod.rs`

```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Subscribe,
    Unsubscribe,
    Cancel { agent_id: Option<String>, session_id: Option<String> },
    Status,
    Kick,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMessage {
    AgentEvent { /* TBD shape — see Notes */ },
    AgentStatus { /* TBD */ },
    DaemonStatus { agents: Vec<AgentSnapshot> },
    Error { message: String },
}

#[derive(Debug, Serialize)]
pub struct AgentSnapshot {
    pub agent_id: String,
    pub session_id: String,
    pub doc_id: String,
    pub elapsed_ms: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
}
```

`agent_event` payload reuses existing `AgentEvent` from `src/engine/runner.rs:32` via `#[serde(flatten)]` or wrapper — pick whichever round-trips cleanly w/ existing `#[derive]`s. Add `Serialize` to `AgentEvent` + `ToolStatus` if absent.

Add `pub mod ipc;` to `src/engine/mod.rs`.

**Verify**: `cargo test -p lazyspec ipc::protocol` passes round-trip tests.

### 2. Newline-delim framing helpers
**ACs**: AC1
**File**: new `src/engine/ipc/framing.rs`

```rust
pub fn write_msg<W: Write, M: Serialize>(w: &mut W, msg: &M) -> Result<()>;
pub fn read_msg<R: BufRead, M: DeserializeOwned>(r: &mut R) -> Result<Option<M>>;
```

`write_msg`: `serde_json::to_writer(w, msg)?; w.write_all(b"\n")?; w.flush()?`.

`read_msg`: `r.read_line(&mut buf)?`. Empty → `Ok(None)` (EOF). Strip trailing `\n`. `serde_json::from_str` → propagate err.

Guard: assert no embedded `\n` in serialised JSON (serde_json default — `to_writer` is single-line for compact).

**Verify**: unit tests in module.

### 3. Broadcaster
**ACs**: AC2, AC3
**File**: new `src/engine/ipc/broadcaster.rs`

```rust
pub struct Broadcaster {
    subs: Arc<Mutex<Vec<Sender<DaemonMessage>>>>,
}

impl Broadcaster {
    pub fn new() -> Self { ... }
    pub fn publish(&self, msg: DaemonMessage);
    pub fn subscribe(&self) -> Receiver<DaemonMessage>;
    pub fn sub_count(&self) -> usize;
}
```

`publish`: lock subs, iter, send. Drop any sub whose send returns `SendError` (rx dropped). Retain live.

`subscribe`: `bounded(64)` or `unbounded()` — pick bounded(256) (back-pressure, drop slow consumer per RFC-041 implicit; if full, drop sub and log). v1 keeps unbounded for simplicity; revisit if memory becomes a concern. Use `crossbeam_channel::unbounded()`.

Clone-cheap: internal `Arc<Mutex<...>>`. Tick loop holds one clone; daemon holds one for handler.

**Verify**: unit tests in module.

### 4. Shared dispatch state
**ACs**: AC4, AC5, AC6
**File**: new `src/engine/ipc/state.rs`

```rust
pub struct DaemonState {
    pub cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>>,  // keyed by BOTH agent_id and session_id
    pub snapshot_provider: Arc<dyn SnapshotProvider>,
    pub broadcaster: Broadcaster,
    pub wake: Sender<()>,  // kick → tick wake
}

pub trait SnapshotProvider: Send + Sync {
    fn snapshot(&self) -> Vec<AgentSnapshot>;
}
```

`cancel_map` populated by tick loop on dispatch — insert both keys mapped to same `Sender<()>`. Removed on agent exit. `SnapshotProvider` impl backed by `TickLoop` state (read-only view of `running: HashMap<String, RunningAgent>`).

Concrete `SnapshotProvider`: thin wrapper over `Arc<Mutex<HashMap<String, RunningAgent>>>` — refactor `TickLoop.running` to `Arc<Mutex<...>>` so the snapshot can be read from the handler thread without crossing into the tick thread. Elapsed = `now - observation.session_started_at` (existing field).

Trait justified: real impl + test fake (`Vec<AgentSnapshot>` recorder).

**Verify**: snapshot returns expected vec for 0 / 1 / N agents.

### 5. Per-conn handler replacing accept_loop stub
**ACs**: AC1, AC2, AC3, AC4, AC5, AC6
**File**: new `src/engine/ipc/handler.rs`, edit `src/engine/daemon.rs:424` (accept_loop)

```rust
pub fn handle_connection(stream: UnixStream, state: Arc<DaemonState>) {
    let read_stream = stream.try_clone().expect("clone unix stream");
    let mut reader = BufReader::new(read_stream);
    let mut writer = stream;

    let mut sub_rx: Option<Receiver<DaemonMessage>> = None;

    loop {
        // Select on: reader has next msg OR sub_rx has next msg to forward.
        // Use non-blocking poll loop w/ short sleep, OR a real select via
        // crossbeam_channel::select! after wrapping reader in a thread.
        // Picked: spawn a reader thread that pushes parsed ClientMessage onto
        // a channel; select! between client_rx and sub_rx.
        ...
    }
}
```

Implementation: spawn a reader thread per conn that loops `read_msg`, pushes `ClientMessage` onto bounded crossbeam channel. Main handler thread `select!` between `client_rx` and `sub_rx` (if subscribed). Write responses on writer.

Dispatch:
- `Subscribe`: `sub_rx = Some(state.broadcaster.subscribe())`.
- `Unsubscribe`: drop sub_rx.
- `Cancel { agent_id, session_id }`: look up key in `cancel_map`. If found, send `()`. If not, write `Error`.
- `Status`: write `DaemonMessage::DaemonStatus { agents: state.snapshot_provider.snapshot() }`.
- `Kick`: `let _ = state.wake.try_send(());` (bounded(1) — full means a kick already pending, no-op).

Handler exit: client disconnects (read returns None or Err) OR write fails (subscriber gone). Reader thread joined on exit. Sub_rx dropped on exit (broadcaster cleans up via send-fail eviction).

Edit `accept_loop` at `daemon.rs:424`:
```rust
fn accept_loop(listener: UnixListener, running: Arc<AtomicBool>, state: Arc<DaemonState>) {
    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let st = Arc::clone(&state);
                thread::spawn(move || handle_connection(stream, st));
            }
            ...
        }
    }
}
```

Wire `state` through `Daemon::run`: build `DaemonState` after `bind_listener`, before spawning accept thread. Pass `state.broadcaster.clone()` + `state.cancel_map.clone()` + `state.wake.clone()` into `TickLoop` via new `TickLoop::with_ipc(broadcaster, cancel_map, wake_rx)` setter.

**Verify**: integration test `framing_over_socket_is_newline_json` connects, sends `status`, reads bounded.

### 6. Tick loop wake channel
**ACs**: AC6
**File**: `src/engine/tick.rs`

Current `run_once` ends w/ `self.clock.sleep(Duration::from_millis(orch.poll_interval_ms))` — blocking, no wake.

Replace w/ interruptible sleep:
1. Add `pub wake_rx: Option<Receiver<()>>` to `TickLoop`.
2. New setter `with_wake(mut self, rx: Receiver<()>) -> Self`.
3. Trait `Clock` adds `fn sleep_interruptible(&self, dur: Duration, wake: &Receiver<()>) -> bool` — returns `true` if woke early. `SystemClock` impl uses `wake.recv_timeout(dur)`. Fake clocks in tests record the call + can inject early wake.
4. `run_once`: if `wake_rx.is_some()`, call `sleep_interruptible(dur, wake_rx)`. Else fall back to existing `sleep(dur)` (preserves test paths that don't wire IPC).

Drain semantics: any pending kick is consumed by the wake (single `recv_timeout`). Multiple kicks during one tick collapse to one wake (bounded(1) wake channel).

**Verify**: new test `tick::kick_wake_interrupts_poll_sleep` — instrument fake clock to send on wake rx mid-sleep, assert `run_once` returns early.

### 7. Cancel-map population from tick loop
**ACs**: AC4
**File**: `src/engine/tick.rs` (dispatch path)

On agent spawn (where `running.insert(doc_id, RunningAgent { ..., cancel, ... })` happens): also insert into shared `cancel_map`:
```rust
let mut map = self.cancel_map.lock().unwrap();
map.insert(agent_ident.clone(), cancel.clone());
map.insert(session_id.clone(), cancel.clone());
```

On agent exit / kill / lease release: remove both keys.

`cancel_map` field added to `TickLoop`: `pub cancel_map: Arc<Mutex<HashMap<String, Sender<()>>>>`. Default `Arc::new(Mutex::new(HashMap::new()))` so existing tests untouched. `with_ipc` setter swaps in the shared one from `DaemonState`.

**Verify**: `ipc::cancel_by_agent_id_signals_cancel_sender` + sibling for session_id.

### 8. Snapshot provider impl
**ACs**: AC5
**File**: `src/engine/tick.rs` or `src/engine/ipc/state.rs`

Concrete `TickSnapshotProvider { running: Arc<Mutex<HashMap<String, RunningAgent>>> }` impls `SnapshotProvider`. `snapshot()` locks, iterates, builds `Vec<AgentSnapshot>` w/ `elapsed_ms = (Instant::now() - obs.session_started_at).as_millis()` (or equivalent existing field — verify via `AgentObservation` struct read).

Refactor `TickLoop.running: HashMap<...>` → `Arc<Mutex<HashMap<...>>>`. Touch every site that mutates `running` (insert / remove / drain on shutdown). Internal lock scopes kept narrow.

**Verify**: `ipc::status_returns_daemon_status_with_running_agents`.

### 9. Wire `DaemonState` into `Daemon::new` and `Daemon::run`
**ACs**: AC2-6
**File**: `src/engine/daemon.rs`

In `Daemon::new`, after constructing tick runner: build `Broadcaster`, `cancel_map`, `wake (tx, rx)`. Pass them into `TickLoop::with_ipc(...)`. Store `DaemonState` on `Daemon` struct.

In `Daemon::run`, pass `Arc::new(state)` clone to `accept_loop`.

Test ctors (`with_lease_releaser`, `with_tick_runner`): default `DaemonState` w/ empty `cancel_map`, empty broadcaster, dummy wake. Keeps existing daemon unit tests green.

**Verify**: full daemon unit test suite still passes.

### 10. README

Skip. Iter A is engine-only; no CLI surface change. README updates land in Iter B w/ `lazyspec daemon status` subcommand.

### 11. `cargo clippy` + `cargo fmt`

`cargo clippy --all-targets --all-features -- -D warnings`. Fix any warnings on touched files.

## Notes

- `send_kick` in `src/cli/assign.rs:77` predates this iter and writes raw `kick\n`. Wire form stays compatible: `ClientMessage::Kick` serialises to `{"type":"kick"}\n`. After this iter lands, the existing `kick\n` (no JSON) will NOT parse — emit `error` msg. Decision: update `send_kick` to write the typed form in the SAME task that lands the handler (task 5) so the kick path doesn't regress. Comment on the function to mark the wire shape.
- Tick loop wake channel: existing `TickLoop::run_until` (`tick.rs:667`) busy-polls shutdown_rx then calls blocking `run_once`. No wake mechanism present. Task 6 adds one via `Clock::sleep_interruptible`. Single concrete `Clock` impl gets a second method; fake clocks in tests need updating (the test fakes are local to `tick.rs::tests`).
- Cancel-map key collisions: agent_id and session_id MUST be distinct strings (guaranteed by RFC-041 — agent_id is `{role}` or similar workflow ident; session_id is per-spawn unique). If a collision ever occurs, last insert wins — acceptable, both keys map to same `Sender<()>`.
- Broadcaster bounded vs unbounded: v1 unbounded. Slow subscriber → memory grows. Slice 7 TUI back-pressure is its own concern. Revisit when first real slow consumer appears.
- `AgentEvent` already derives `Debug, Clone, PartialEq, Eq` — add `Serialize` (no `Deserialize` needed; daemon emits only). `ToolStatus` same.
- No premature trait extraction: `Broadcaster` is concrete struct, not trait. `SnapshotProvider` IS a trait (two concrete uses justified per dictum 6: real `TickSnapshotProvider` + test fake). `MessageHandler` is a free function (`handle_connection`); no trait until a second handler shape exists.
- I/O at seams (dictum 4): tests use real `UnixStream` pairs from real bound `UnixListener` in `TempDir`. No mock streams. The seams are typed protocol enums + `SnapshotProvider` trait + cancel/wake channels — natural types, not bespoke fakes.
- Reader thread per conn: each conn spawns one reader thread that parses `ClientMessage`s onto a channel, freeing the main handler thread to `select!` between client msgs and forwarded subscriber events. Reader thread exits on stream close; handler joins it.
- Iter B (AC7-9) plugs `lazyspec daemon status [--json]` CLI on top of this protocol + adds subscriber reconnect helper for TUI use.
