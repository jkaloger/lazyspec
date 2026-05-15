---
title: Daemon status CLI and subscriber reconnect
type: iteration
status: draft
author: agent
date: 2026-05-15
tags: []
related:
- implements: STORY-123
---

## Goal

STORY-123 slice B. Ship `lazyspec daemon status [--json]` thin client + reconnecting subscriber helper in `engine::ipc::client`. Consume Iter A's `engine::ipc` types (client/daemon msgs, newline-JSON framing, daemon-side handler).

## In Scope

- ACs 7,8 — `lazyspec daemon status [--json]` subsubcmd. Connect UDS `.lazyspec/daemon.sock`, send `status` msg, read single `daemon_status` reply, print snapshot (`--json` raw JSON, else table), exit 0.
- AC8 — absent-daemon path. `ConnectionRefused` / `NotFound` on connect → stderr `"daemon not running"`, exit non-zero (code 1). No spawn. No fork. No retry.
- AC9 — reconnecting subscriber helper in `engine::ipc::client`. Lib fn returns iter/stream of `DaemonEvent`. On transient disconnect (EOF, broken pipe, ECONNREFUSED): backoff + reconnect + resend `subscribe`. Consumer is STORY-122 TUI (slice 7).

## Out of Scope

- AC1 protocol msg types + newline-JSON framing — Iter A.
- AC2 daemon-side accept loop + subscribe streaming — Iter A.
- AC3 fan-out broadcaster — Iter A.
- AC4 cancel routing → SIGTERM — Iter A.
- AC5 daemon-side status snapshot source — Iter A.
- AC6 kick wake from poll wait — Iter A.
- TUI rendering — slice 7 (STORY-122).
- Windows named-pipe transport — deferred beyond v1.
- Auth / authz on socket — Unix perms only, no v1 scope.

## Test Plan

- **AC7** — integration `tests/cli_daemon_status_test.rs::daemon_status_prints_snapshot`. Spawn Iter A daemon (UDS bound, status handler installed) into `TempDir` workspace. Call CLI logic (`crate::cli::daemon::status::run`) w/ `--json`. Capture stdout. Parse via `serde_json::from_str::<DaemonStatus>(...)`. Assert exit 0, agent-list shape (Vec of `{ agent_id, doc_id, elapsed_ms, tokens_in, tokens_out }`). Seed daemon w/ zero agents → empty Vec. Bounded chan ack on daemon-side handler completion; no real-time sleeps.
- **AC8** — integration `tests/cli_daemon_status_test.rs::daemon_status_absent_daemon`. `TempDir` workspace, no `.lazyspec/daemon.sock` bound. Run CLI logic. Assert (a) non-zero exit code, (b) stderr contains `daemon not running`, (c) stdout empty, (d) no socket file created post-call, (e) no child process spawned. Second case: socket file exists but no listener (stale) → same outcome (maps to `ConnectionRefused`).
- **AC9** — integration `tests/ipc_subscriber_reconnect_test.rs::subscriber_resubscribes_after_daemon_restart`. Bind ephemeral daemon (Iter A handler) on `TempDir` UDS path. Start subscriber helper → asserts first event delivered via bounded chan w/ explicit timeout deadline. Drop daemon listener (simulates crash). Rebind on same path. Inject a fresh event from new daemon. Assert subscriber yields it w/o caller intervention. Inject `Sleeper` test seam (`fn sleep(d: Duration)`) recording call durations; assert backoff schedule (e.g. `[100ms, 200ms, 400ms]` capped) — no wall-clock sleeps. Use `crossbeam_channel::after` shim or pure fn injection.
- **AC9 unit** — `src/engine/ipc/client.rs::tests::reconnect_resends_subscribe`. Mock `Connector` trait seam returning scripted streams (first stream EOFs after one event, second stream yields second event). Assert subscriber writes `subscribe\n` framed msg on each new connection.
- **AC9 unit** — `src/engine/ipc/client.rs::tests::permanent_error_propagates`. Non-transient err (e.g. `PermissionDenied`) → helper returns err, no infinite retry.

Tradeoffs: bounded chan + deadline > real timeouts. `Sleeper` trait is a real seam (dictum 4) — backoff timing is I/O. No `tokio::time::pause` since repo is sync. No threads other than the helper's own.

## Changes

1. **`engine::ipc::client` module skeleton** — AC9. New file `src/engine/ipc/client.rs`. Re-export from `src/engine/ipc/mod.rs` (Iter A owns the mod root). Types:
   - `pub trait Connector: Send + 'static { fn connect(&self) -> io::Result<UnixStream>; }`
   - `pub struct SocketConnector { path: PathBuf }` impl `Connector` via `UnixStream::connect`.
   - `pub trait Sleeper: Send + 'static { fn sleep(&self, d: Duration); }`
   - `pub struct ThreadSleeper;` impl via `std::thread::sleep`.
   - `pub struct ReconnectingSubscriber<C: Connector, S: Sleeper> { connector: C, sleeper: S, backoff: BackoffSchedule }`.
   - `BackoffSchedule { base_ms: u64, cap_ms: u64, attempt: u32 }` w/ `fn next(&mut self) -> Duration` — exponential `min(base*2^n, cap)`.
   - `pub fn events(self) -> Receiver<DaemonEvent>` — spawns owner thread, returns `crossbeam_channel::Receiver`. Thread loop: connect → write `subscribe\n` framed → read newline-JSON loop → on EOF/ECONNREFUSED/BrokenPipe → sleep next backoff → reconnect. On non-transient err → send `Err` event variant + close. On caller drop of Receiver → thread exits next loop iter (detect via `send` returning `SendError`).
   Verify: `cargo test -p lazyspec --lib engine::ipc::client`.

2. **Transient err classification** — AC9. In `client.rs` add `fn is_transient(e: &io::Error) -> bool` — matches `ConnectionRefused`, `ConnectionReset`, `BrokenPipe`, `UnexpectedEof`, `NotFound` (daemon yet to bind). Anything else → permanent. Unit-tested w/ table of `ErrorKind` cases.
   Verify: `cargo test -p lazyspec --lib engine::ipc::client::tests::is_transient_classifies`.

3. **`daemon status` subsubcmd in CLI tree** — ACs 7,8. File `src/cli.rs`. Current `Daemon` is unit variant (line 349). Convert to grouped subcmd:
   ```
   Daemon { #[command(subcommand)] cmd: Option<DaemonCmd> }
   enum DaemonCmd { Status { #[arg(long)] json: bool } }
   ```
   `None` → existing run-daemon path (preserve `lazyspec daemon` bare = foreground run, back-compat). `Some(Status { json })` → new path. Verify clap renders help correctly: `cargo run -- daemon --help` shows status subcmd.
   Update `src/main.rs` dispatch: match new shape, route `Status` to `crate::cli::daemon::status::run`.
   Verify: `cargo run -- daemon --help` lists `status`. `cargo run -- daemon status --help` shows `--json`.

4. **`cli::daemon::status` module** — ACs 7,8. Promote `src/cli/daemon.rs` → `src/cli/daemon/mod.rs` (keep existing `run` fn for foreground cmd) + add `src/cli/daemon/status.rs`. Sig:
   ```
   pub fn run(root: &Path, json: bool) -> Result<(), DaemonStatusError>
   ```
   Impl: `let path = root.join(DAEMON_SOCKET);` (use existing const from `src/cli/assign.rs:75` — relocate to `src/engine/ipc/mod.rs` if not already moved by Iter A; otherwise re-export). `UnixStream::connect(&path)` →
   - `Err(e) if e.kind() in {ConnectionRefused, NotFound}` → return `DaemonStatusError::NotRunning`.
   - `Ok(mut s)` → write `{"type":"status"}\n` (use Iter A's `ClientMessage::Status` serialize via framing helper). Read one newline-delimited reply. Parse via Iter A's `DaemonMessage::DaemonStatus { agents: Vec<AgentSnapshot> }`. Print:
     - `--json` → `serde_json::to_string(&snapshot)` to stdout, newline.
     - else → tabular: header `AGENT  DOC  ELAPSED  TOKENS-IN  TOKENS-OUT`, one row per agent.
   - Return `Ok(())`.
   No spawn. No retry. Single round-trip.
   Verify: `cargo test --test cli_daemon_status_test`.

5. **Exit-code semantics for absent daemon** — AC8. Avoid `std::process::exit` inside library code (kills test harness). Define `pub enum DaemonStatusError { NotRunning, Io(io::Error), Protocol(serde_json::Error) }` in `cli/daemon/status.rs`. `run` returns `Result<(), DaemonStatusError>`. Bridge in `src/main.rs`: `Err(DaemonStatusError::NotRunning)` → `eprintln!("daemon not running")` + `std::process::exit(1)`. Other errs → bubble via anyhow. Tests call `run` directly + assert err variant — no subprocess needed.
   Verify: AC8 test asserts `matches!(err, DaemonStatusError::NotRunning)`.

6. **README + help update** — ACs 7,8. File `README.md`. Existing CLI table / Daemon section: add `lazyspec daemon status [--json]` row + short blurb. Update `src/cli.rs` doc-comment on `Daemon` to mention `status` subcmd. Per project rule: keep README in sync.
   Verify: `cargo run -- help daemon` shows status; `grep "daemon status" README.md`.

7. **Integration tests** — all ACs. Two files:
   - `tests/cli_daemon_status_test.rs` — ACs 7,8. Helpers: `spawn_minimal_daemon(td) -> (handle, sock_path)` using Iter A's `engine::ipc::serve` (or whatever it exposes) on a `TempDir` UDS path. Tests: `daemon_status_prints_snapshot`, `daemon_status_json_shape`, `daemon_status_absent_daemon`, `daemon_status_stale_socket_treated_as_absent`.
   - `tests/ipc_subscriber_reconnect_test.rs` — AC9. Tests: `subscriber_resubscribes_after_daemon_restart`, `subscriber_yields_events_post_reconnect`, `subscriber_terminates_on_permanent_error`, `subscriber_exits_when_receiver_dropped`.
   Use `TempDir` for socket paths (isolated). Bounded chans + `recv_timeout` w/ generous deadline (e.g. 2s) — deterministic w/o real sleeps in helper code.
   Verify: `cargo test --test cli_daemon_status_test --test ipc_subscriber_reconnect_test`.

8. **Clippy + validate** — all ACs. Run `cargo clippy --all-targets -- -D warnings`. Run `cargo run -- validate --json`. Fix any new warnings.
   Verify: clean output.

## Notes

Depends on Iter A landing first:
- `engine::ipc::{ClientMessage, DaemonMessage, AgentSnapshot, DaemonStatus}` serde types (tagged enums, newline-JSON framed).
- Iter A daemon-side `status` handler returning `DaemonMessage::DaemonStatus`.
- Iter A daemon-side `subscribe`/`agent_event` broadcaster (needed for AC9 reconnect test event injection).
- Iter A framing helpers (`read_msg<R>(r) -> io::Result<Option<T>>`, `write_msg<W>(w, &m)`).

If Iter A names types differently, adjust task 4/test seams accordingly — protocol shape per RFC-041 §"IPC and CLI surface" is the contract.

Dictum-2: `--json` honored on `daemon status`. Default human format is tabular (consistent w/ other status-style cmds).

Dictum-3 (engine vs CLI split): reconnect helper + framing live in `engine::ipc::client`. CLI side is glue: parse args, call lib, format output. The single round-trip in `status::run` stays in CLI module since it is the entire feature surface — no library re-use case.

Dictum-4 (I/O at seams): `Connector` + `Sleeper` traits introduced **only** to make AC9 testable w/o real sockets/sleeps. Single prod impl each (`SocketConnector`, `ThreadSleeper`) + single test mock — meets dictum-6 (≥2 uses: prod + test).

Dictum-6 (no premature trait): no trait for status round-trip — single concrete fn, single call site.

CLI subcmd shape: `lazyspec daemon` (bare) stays the foreground-run cmd (back-compat w/ ITERATION-171). `lazyspec daemon status` is new subsubcmd. clap derive supports this via `#[command(subcommand)] cmd: Option<DaemonCmd>` on the variant — `None` → run, `Some(Status)` → status.

`DAEMON_SOCKET` const currently in `src/cli/assign.rs:75`. Iter A likely relocates to `src/engine/ipc/mod.rs` (or `src/engine/daemon.rs` already has `SOCKET_REL_PATH` const — prefer single source of truth). Coordinate w/ Iter A; if not moved, this iter relocates as task-0 grunt-work.

Tests deterministic: no `std::thread::sleep`, no real-time deadlines except generous `recv_timeout` upper bounds. Backoff is exercised via injected `Sleeper` recording call args, not via wall clock.
