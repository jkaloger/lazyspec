---
title: "Daemon dispatch observability"
type: iteration
status: draft
author: "agent"
date: 2026-05-15
tags: []
related: []
---

## In Scope

- Broadcast pre-spawn dispatch failures (lease acquire, branch render, workspace provision, agent spawn) over IPC `DaemonMessage::Error`.
- Surface daemon errors in TUI as a transient toast strip (header banner above status bar, auto-clears).
- Info-level structured logs at tick boundaries + dispatch-stage transitions + IPC kick receipt. Done via `eprintln!` w/ consistent prefix (no new crate); each line carries `tick_id` + monotonic ms-since-start so kick→action latency is measurable from daemon logs alone.

## Out of Scope

- Workspace idempotency fix (ITERATION-184 — separate, already drafted).
- Switching to `tracing` crate (larger refactor; defer until 3rd consumer per dictum 6).
- Persisting toast history / error log file.
- Rendering `KickoffFeedback::AssignedAndKicked` / `AssignedOnly` after picker closes (pre-existing UI gap, separate).
- Caveman-mode kick latency root cause: this iter gives the logs to find it; fix is a follow-up iter if logs reveal an actual bug.

## Acceptance Criteria

**AC1: dispatch failure → broadcaster Error event**

Given tick loop hits a pre-spawn failure (lease acquire, branch render, workspace provision, or runner spawn)
When the failure path runs
Then `DaemonMessage::Error { message }` is published to `broadcaster`
And `message` contains the doc_id + stage name + underlying error text
And the lease is still released (no behavior regression vs current `continue`)

**AC2: TUI displays daemon Error as toast**

Given the TUI is subscribed to the daemon
When the daemon publishes `DaemonMessage::Error { message }`
Then `app.toast` (new field) carries `Toast { message, expires_at }`
And `draw_status_bar` (or a sibling region) renders the toast text in a distinct style above the status line
And the toast clears after a fixed TTL (5 seconds) or on next keystroke

**AC3: daemon tick lifecycle logs**

Given the daemon tick loop runs with orchestration enabled
When a tick begins, dispatches an agent, or finishes its poll wait
Then `eprintln!` lines of the form `daemon: tick=<n> t=<ms> <event> <kv-pairs>` are emitted for: tick start, candidates loaded (count), each dispatch stage (lease_acquire / branch_render / workspace_provision / spawn) success or failure, sleep start, kick received (wake fired), sleep wake (interrupted vs timeout)
And the `tick=<n>` counter is monotonic per daemon process
And `t=<ms>` is monotonic ms since daemon `Daemon::run` entry

**AC4: kick path logs**

Given the daemon receives `ClientMessage::Kick` over IPC
When the handler dispatches the message
Then a log line `daemon: ipc kick received` is emitted before `state.wake.try_send`
And if `try_send` returns `Err(TrySendError::Full)` a `daemon: ipc kick dropped (channel full)` line is emitted

**AC5: no regression in existing eprintln callers**

Given existing failure-cap and reconcile call sites (`tick.rs:903`, `:1018`, `:1034`, `:984`, `:524`, `:543`)
When the new logs land
Then existing call sites either remain or are rewritten to the new prefix without changing semantics
And `cargo test --lib` passes
And `cargo clippy --all-targets -- -D warnings` is clean

## Test Plan

DICTUM-004: real types over mocks; trait seams at I/O.

**AC1 tests** (`src/engine/tick.rs` `#[cfg(test)] mod tests`):

- `provision_failure_publishes_error_event` — fake `WorkspaceProvisioner` whose `provision` returns `Err`. Build `TickLoop` with `with_broadcaster(bc)`; subscribe before tick. Seed one eligible candidate via lease-fetch fake. Run `run_once`. Assert: `bc` subscriber receives one `DaemonMessage::Error`, `message` contains doc id + `"workspace provision"` + the underlying err string.
- `spawn_failure_publishes_error_event` — fake `AgentRunner` whose `spawn` returns `Err`. Same harness. Assert error event published.
- `branch_render_failure_publishes_error_event` — set `orch.branch_template` to a template that fails (e.g. references unknown var). Same harness. Assert error event.
- `lease_acquire_failure_publishes_error_event` — fake `LeaseOps` whose `acquire` returns `Err`. Same harness. Assert error event.

Properties: each test owns a fresh `TickLoop` + `Broadcaster`; assertions on the published `DaemonMessage` (behavioral), not on `eprintln` capture.

**AC2 tests** (`src/tui/state/agents.rs` and/or `src/tui/state/app.rs`):

- `agents_view_apply_error_records_toast` — call `app.apply_daemon_error("...")` (new pathway routed from `event_loop.rs`); assert `app.toast` is `Some(_)` with message + future `expires_at`.
- `toast_expires_after_ttl` — drive a fake clock; advance past TTL; call `app.tick_toast(now)`; assert `app.toast` is `None`.

Properties: deterministic via injected clock; no real `Instant::now`.

**AC3 tests**:

Skipped — `eprintln!` content is not behavioral and DICTUM-004 discourages stdout capture in unit tests. Verification: manual + `lazyspec daemon 2>&1 | tee /tmp/daemon.log` smoke run documented in iteration Notes.

**AC4 tests**:

- `kick_handler_emits_log_and_wakes` — existing `handle_connection` tests already cover the wake side. Add: assert `try_send` is invoked. (eprintln verification deferred to manual smoke.)

**AC5**:

- Run full `cargo test --lib`. Run `cargo clippy --all-targets -- -D warnings`.

## Changes

### Task 1: `DaemonMessage::Error` broadcast helper

ACs: AC1.

Files:
- `src/engine/tick.rs`: add private helper `fn publish_dispatch_error(&self, doc_id: &str, stage: &str, err: &dyn std::fmt::Display)` on `TickLoop`. Body:
  1. `eprintln!("daemon: tick=… t=… dispatch_failed doc={} stage={} err={}", …)` (consistent prefix per AC3).
  2. If `self.broadcaster.is_some()`: build `DaemonMessage::Error { message: format!("{doc_id}: {stage}: {err}") }` and publish.

- Replace the four pre-spawn `eprintln!` + `continue` sites in `run_once`:
  - line ~684 (lease_acquire) → call helper w/ stage `"lease_acquire"`.
  - line ~703 (branch render) → stage `"branch_render"`.
  - line ~720 (workspace provision) → stage `"workspace_provision"`.
  - line ~742 (spawn) → stage `"spawn"`.

Lease release behavior at each site is unchanged (already in the `Err` arms today; helper only fans out the message).

Verify: `cargo test tick -- --nocapture` for the four new tests above.

### Task 2: TUI toast field + render

ACs: AC2.

Files:
- `src/tui/state/app.rs`:
  - Add `pub toast: Option<Toast>` on `App`. Define `pub struct Toast { pub message: String, pub expires_at: Instant }` co-located.
  - Add `pub fn set_toast(&mut self, message: String)` (TTL = 5s, uses `Instant::now() + Duration::from_secs(5)`).
  - Add `pub fn tick_toast(&mut self, now: Instant)` that clears if `now >= expires_at`.
- `src/tui/infra/event_loop.rs:237` (`AppEvent::AgentsDaemonMessage`): pre-dispatch peek for `DaemonMessage::Error { message }` → call `app.set_toast(message.clone())` before forwarding to `agents_view.apply`. (Apply still no-ops on Error, intentional.)
- `src/tui/views/status_bar.rs`: render `app.toast` if present as a single-line strip styled `Style::default().fg(Color::Red).bg(Color::DarkGray)` above the existing status line. Layout: split current status area `Constraint::Length(1) | Constraint::Min(0)` only when toast is `Some`.
- Top of event loop iteration: `app.tick_toast(Instant::now())` to evict expired.
- On any `AppEvent::Terminal(_)` key press: `app.toast = None` (dismiss-on-input).

Tests: per Test Plan AC2.

Verify: `cargo test app tui::state -- --nocapture` + manual smoke (run daemon, fail provision, observe red banner).

### Task 3: Tick lifecycle logs

ACs: AC3, AC5.

Files:
- `src/engine/tick.rs`:
  - Add fields `pub tick_id: u64` (counter) + `pub started_at: Option<Instant>` on `TickLoop`. Initialize `started_at` lazily on first `run_once` call (`SystemClock::now_instant`).
  - Helper `fn log(&mut self, event: &str, kv: &str)` that prints `daemon: tick={} t={}ms {} {}`.
  - At top of `run_once`: increment `tick_id`, log `tick_start`.
  - After `load_candidates`: log `candidates loaded count=<n> selected=<m>`.
  - In each pre-spawn dispatch stage: log success `dispatch_stage_ok doc=<id> stage=<name>` (kept terse; failures go through Task 1's helper).
  - Before sleep: log `sleep_start pace_ms=<n>`.
  - After sleep_interruptible: log `sleep_wake interrupted=<bool>`.
  - Replace bare `eprintln!` lines at 524, 543, 624 with the helper or leave w/ unified prefix.

Verify: manual `cargo run -- daemon 2>&1 | head -50` — confirm logs include `tick=` + monotonic `t=`. `cargo clippy --all-targets -- -D warnings`.

### Task 4: IPC kick log

ACs: AC4, AC5.

Files:
- `src/engine/ipc/handler.rs` `ClientMessage::Kick` branch (line 143):
  - Before `state.wake.try_send`: `eprintln!("daemon: ipc kick received")`.
  - If `try_send` returns `Err(TrySendError::Full)`: `eprintln!("daemon: ipc kick dropped (channel full)")`. (Currently the result is discarded — switch to a `match`.)

Verify: existing `handler` tests unaffected (they don't assert stderr). `cargo test --lib`.

### Task 5: Sanity sweep

ACs: AC5.

Verify only:
- `cargo test --lib`
- `cargo clippy --all-targets -- -D warnings`
- Manual: start daemon, kick from TUI on STORY-125 (with existing worktree), observe (a) red toast in TUI w/ "workspace provision" stage, (b) daemon logs showing kick receipt, tick start, dispatch stage failure, sleep wake.

## Notes

- ITERATION-184 fixes the underlying root cause (idempotent `provision_workspace`). This iter is the *observability* slice — it makes future failures of any kind diagnosable from the daemon log + TUI. Both should land before STORY-125 is re-attempted.
- Defer `tracing` crate adoption until a third structured-logging consumer appears (per principle 6). Until then, a consistent `daemon: tick=… t=… <event> <kv>` prefix is enough for `grep` + tools like `lnav`.
- Toast TTL chosen at 5s as a "long enough to read, short enough not to clutter". Hard-coded — promote to config only on user request.
- Kick latency diagnosis path: with AC3+AC4 logs landed, reproduction trace will be: `ipc kick received` (handler) → `sleep_wake interrupted=true` (tick) → `tick_start tick=N` → `candidates loaded` → `dispatch_stage_ok` or `dispatch_failed`. If `sleep_wake` is more than ~50ms after `ipc kick received`, the wake channel or `recv_timeout` is suspect. If `sleep_wake interrupted=false` precedes the kick log, the kick missed the sleep window entirely — likely a race between TUI assignee write commit and daemon's `load_candidates`.
- DICTUM-003 layering: TUI must not depend on engine internals beyond the existing `DaemonMessage` protocol. Toast field on `App` is TUI-only. Engine emits `DaemonMessage::Error`; TUI decides presentation.
- DICTUM-004 testing: pre-spawn failure tests use the existing `FakeProvisioner` pattern (line 1511) for the provisioner seam; broadcaster `Broadcaster::subscribe` returns a `Receiver` that the test polls with a short `recv_timeout`. No sleeps, no real git.
