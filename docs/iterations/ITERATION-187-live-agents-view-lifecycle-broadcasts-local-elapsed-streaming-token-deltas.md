---
title: "Live agents view: lifecycle broadcasts, local elapsed, streaming token deltas"
type: iteration
status: accepted
author: "agent"
date: 2026-05-19
tags: [tui, daemon, ipc, agents]
related: []
---

## Context

Standalone bugfix. No parent Story.

TUI agents view sluggish. Root cause: `DaemonMessage::DaemonStatus` only published at end of `run_once`, paced by `orch.poll_interval_ms` (default 30s, no override in `.lazyspec.toml`). All snapshot fields rendered by TUI (`elapsed_ms`, `tokens_in`, `tokens_out`, presence/absence in list) are frozen between broadcasts.

Symptoms → cause:

| Symptom | Mechanism |
|---|---|
| spawn invisible 30s | new agent inserted into `running` mid-tick; only end-of-tick broadcast carries it; next tick is 30s later if tick already broadcast |
| slow updates | snapshot map replaced once per 30s tick |
| tokens stale | only `TurnCompleted` updates tokens; mid-turn streaming has no token signal |
| elapsed slow | `elapsed_ms` baked at broadcast; TUI never re-derives |

Evidence: `engine/tick.rs:940-948` (end-of-tick publish), `engine/tick.rs:1472-1494` (`snapshot_running`), `tui/state/agents.rs:82-91` (`apply(DaemonStatus)` is only path that mutates `snapshots`), `tui/views/panels.rs:1487-1506` (renders `elapsed_ms`/tokens straight from snapshot), `engine/runner/stream.rs:93-101` (tokens parsed only from `result` record).

## Acceptance Criteria

- **AC1 spawn-visible-fast.** Given daemon spawns agent for doc X. When agent inserted into `running` map. Then `DaemonStatus` published immediately (same tick, before sleep) carrying X. TUI snapshot map contains X within one event-loop iteration of receive.
- **AC2 termination-visible-fast.** Given agent for doc X in `running`. When tick removes it (retry kill, shutdown drain, lease release path). Then `DaemonStatus` published immediately after removal. TUI snapshot map drops X within one event-loop iteration.
- **AC3 elapsed-smooth.** Given snapshot for session S received at TUI time T0 with `elapsed_ms = E0`. When TUI renders at time T1. Then displayed elapsed = `E0 + (T1 - T0)` in ms, recomputed every frame. No reliance on next `DaemonStatus` to advance.
- **AC4 mid-turn-token-deltas.** Given claude `assistant` stream record with `message.usage = {input_tokens: I, output_tokens: O}`. When parsed. Then a `TokenUsage { I, O }` event is emitted in addition to the primary `Text` / `ToolCallStarted` event. `AgentsViewState::apply(TokenUsage)` updates the corresponding snapshot's `tokens_in`/`tokens_out`.
- **AC5 no-regression-existing-broadcasts.** Existing end-of-tick `DaemonStatus` (tick.rs:940) still fires. `Subscribe` seed still uses `snapshot_provider.snapshot()`. Existing `TurnCompleted` still updates tokens. All current tick/agents/ipc tests pass.

## Changes

### 1. `AgentEvent::TokenUsage` variant + Vec-returning parse

Files: `src/engine/runner.rs`, `src/engine/runner/stream.rs`, `src/engine/runner/claudep.rs`.

ACs: AC4.

Detail:

- `runner.rs`: add variant
  ```rust
  TokenUsage { input_tokens: u64, output_tokens: u64 },
  ```
  Serde tag `token_usage` (snake_case derive already in place).
- `stream.rs`: change `parse_record(line: &str) -> Option<AgentEvent>` → `pub(crate) fn parse_record(line: &str) -> Vec<AgentEvent>`. For `"assistant"` records:
  - Run existing `parse_assistant_text` → push primary if `Some`.
  - Also extract `message.usage.input_tokens` + `output_tokens`; if both present, push `TokenUsage`.
  - Order: primary first, then `TokenUsage`. Stable for tests.
- For other record types: return single-element Vec (or empty).
- `claudep.rs:50`: iterate Vec, send each.
- `tui/state/agents.rs` (`AgentsViewState::apply`): handle `AgentEvent::TokenUsage` by updating `snap.tokens_in`/`tokens_out` on existing snapshot. If snapshot absent, create stub (mirror `TurnCompleted` behavior at agents.rs:58-71).

Verification:
- `cargo test -p lazyspec runner::stream::tests` — new + existing.
- `cargo test agents` — TokenUsage apply test.

### 2. Lifecycle broadcasts in tick loop

Files: `src/engine/tick.rs`.

ACs: AC1, AC2.

Detail:

- Extract helper:
  ```rust
  fn broadcast_status(&self) {
      if let Some(bc) = self.broadcaster.as_ref() {
          bc.publish(DaemonMessage::DaemonStatus {
              agents: snapshot_running(&self.running),
          });
      }
  }
  ```
- Call sites:
  - tick.rs ~919-938: after `self.running.lock().unwrap().insert(...)` for newly-spawned agent + after `cancel_map` insert, call `self.broadcast_status()`. Drops in-tick before existing end-of-tick publish — same data on rapid path so cheap.
  - tick.rs `kill_agent_for_retry` (~996): after lease release + map cleanup, broadcast.
  - tick.rs `run_until` shutdown drain (~973-993): after draining all running agents and releasing leases, broadcast once (now empty snapshot). Subscribers see empty list before daemon teardown.
  - Anywhere else `self.running` shrinks (reconcile path that finds dead session). Grep for `running.lock().unwrap().remove` after impl.
- Keep existing end-of-tick broadcast at tick.rs:940 untouched (AC5).

Verification:
- New tick test: spawn fake agent → assert broadcaster received `DaemonStatus` carrying agent before `clock.sleep` called.
- New tick test: `kill_agent_for_retry` → assert broadcaster receives `DaemonStatus` without that doc_id.
- `cargo test --features agent` full pass.

### 3. TUI-side local elapsed extrapolation

Files: `src/tui/state/agents.rs`, `src/tui/views/panels.rs`.

ACs: AC3.

Detail:

- `AgentsViewState`: add field
  ```rust
  pub synced_at: HashMap<String, std::time::Instant>,
  ```
- `apply(DaemonStatus { agents })` (agents.rs:82): when rebuilding `snapshots`, also record `synced_at.insert(session_id, Instant::now())` for each agent in payload. Retain only keys present in new snapshot map (parallel to existing `output`/`statuses` pruning).
- `apply(TokenUsage)`, `apply(TurnCompleted)`: do NOT update `synced_at`; elapsed extrapolation continues from last `DaemonStatus` baseline.
- Add helper:
  ```rust
  pub fn effective_elapsed_ms(&self, session_id: &str) -> Option<u64> {
      let snap = self.snapshots.get(session_id)?;
      let baseline = snap.elapsed_ms;
      let delta = self.synced_at.get(session_id)
          .map(|t| t.elapsed().as_millis() as u64)
          .unwrap_or(0);
      Some(baseline + delta)
  }
  ```
- `panels.rs:1487-1506`: replace `snap.elapsed_ms` with `app.agents_view.effective_elapsed_ms(&snap.session_id).unwrap_or(snap.elapsed_ms)`.
- `load_offline` (agents.rs:96): do not populate `synced_at` so offline rows show snapshot-time elapsed only. Offline behavior unchanged.
- `set_connection(Connected)` (agents.rs:115): clear `synced_at` alongside `snapshots.clear()`.

Verification:
- Unit test: `apply(DaemonStatus)` records `synced_at`. Then `effective_elapsed_ms` after a ~50ms sleep returns `>= baseline + 50`. Tolerance window for CI.
- Unit test: `effective_elapsed_ms` returns `None` for unknown session.
- Unit test: offline sessions have `effective_elapsed_ms` returning baseline only (no extrapolation).

### 4. CLI/README touch-ups

Files: `README.md` if any agents-view docs reference timing; otherwise skip.

ACs: AC5.

Detail: grep `README.md` for `poll_interval_ms`, `agents view`, `elapsed`. If any prose claims update cadence ≥ 1s, correct it. Per CLAUDE.md: keep README in sync with CLI surface; no CLI surface change here so likely no-op.

Verification: grep returns nothing relevant, or edits show correct cadence.

## Test Plan

### Unit (engine)

- `runner::stream::tests::assistant_with_usage_emits_text_and_token_usage` — assistant record with both text and usage → `vec![Text, TokenUsage]` in that order.
- `runner::stream::tests::assistant_with_usage_only_emits_token_usage` — record carrying usage but no parseable text/tool_use block → `vec![TokenUsage]`.
- `runner::stream::tests::tool_use_with_usage_emits_tool_call_started_and_token_usage` — tool_use precedence preserved, plus usage.
- `runner::stream::tests::assistant_missing_usage_emits_primary_only` — back-compat.
- Existing `parses_turn_complete_with_usage`, `parses_assistant_text_delta`, etc. updated to expect Vec (single-elem) rather than `Option`.

### Unit (tick)

- `tick::tests::spawn_broadcasts_daemon_status_before_sleep` — fake broadcaster + fake clock; after `run_once` spawns agent, broadcaster received a `DaemonStatus` containing the agent before `clock.sleep` recorded.
- `tick::tests::kill_agent_for_retry_broadcasts_removal` — pre-seed running map, call `kill_agent_for_retry`, assert broadcaster's last `DaemonStatus` excludes the doc_id.
- `tick::tests::shutdown_drain_broadcasts_empty_status` — pre-seed running, call `run_until` with immediate shutdown signal, broadcaster's last message has `agents: []`.

### Unit (TUI agents state)

- `agents::tests::apply_daemon_status_records_synced_at` — after apply, `synced_at` contains exactly the session ids in payload.
- `agents::tests::effective_elapsed_ms_extrapolates_local_time` — apply snapshot with `elapsed_ms = 100`, sleep ~50ms (`thread::sleep`), assert `effective_elapsed_ms >= 150`.
- `agents::tests::apply_token_usage_updates_snapshot_tokens` — pre-seed snapshot via `DaemonStatus`, apply `TokenUsage { 7, 11 }`, assert snapshot tokens = (7, 11) and `synced_at` unchanged.
- `agents::tests::set_connection_connected_clears_synced_at` — populate `synced_at`, flip to Connected, assert cleared alongside snapshots.

### Test property tradeoffs

- Tick lifecycle tests use the existing `FakeClock` + `Broadcaster` seam (fully deterministic, no sleeps).
- `effective_elapsed_ms_extrapolates_local_time` uses real `Instant::now()` + a short sleep. Non-deterministic on stressed CI. Acceptable because: (a) only one assertion uses real time, (b) lower bound (`>=`) tolerates slow scheduling, (c) trait-mocking `Instant` for one helper crosses the cost/benefit line per principle 6.
- Stream parsing tests stay pure parse → no fixtures.

## Notes

- Existing `synced_at` analog in codebase: none. New field. No abstraction needed yet (principle 6).
- `DaemonMessage` wire format unchanged. `AgentSnapshot` unchanged. Adds a new `AgentEvent` variant only (`token_usage`).
- Forward-compat for IPC subscribers that don't know `token_usage`: serde will fail to deserialize. Acceptable for in-tree; no out-of-tree consumers documented. If discovered, gate behind feature.
- `poll_interval_ms` itself stays at 30s. Dispatch cadence is correct for git scans + claude API rate; only the *observability* cadence was wrong.
- `synced_at` uses `Instant::now()` (monotonic). Survives wall-clock skew but not process restart — fine, TUI doesn't restart mid-session of interest.
- Alternative considered: send `started_at_unix_ms` in `AgentSnapshot`, TUI computes from wall clock. Rejected: wider wire change, no benefit over monotonic local extrapolation given snapshot is refreshed at least once per 30s.
