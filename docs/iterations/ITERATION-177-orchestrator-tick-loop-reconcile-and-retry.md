---
title: Orchestrator tick loop reconcile and retry
type: iteration
status: accepted
author: agent
date: 2026-05-13
tags: []
related:
- implements: STORY-128
- blocks: ITERATION-178
---

## Scope

Iter B of STORY-128. AC8-14. Depends on Iter A (`ITERATION-176`): TickLoop, Dispatcher, Clock, lease integration, running map.

In: stream-json event consumption per running agent, stall detection w/ tool_use suspension, hard turn timeout, doc-status reconcile (terminal/handoff/active), clean-exit continuation w/ `max_turns`, failure backoff w/ `max_failure_attempts`, post-cap release + `failed` event emit.

Out: boot orphan recovery + preflight (Iter C). IPC client `agent_event` socket multiplexing (slice 6). Doc status mutation by daemon (daemon NEVER mutates status — per RFC-041).

## Changes

### 1. Extend `OrchestrationConfig` w/ reconcile/retry fields
**ACs**: AC8, AC9, AC12, AC13, AC14
**File**: `src/engine/config.rs`

Add to `OrchestrationConfig`:

- `stall_timeout_ms: u64` (default 300000 = 5min)
- `max_turns: u32` (default 20)
- `max_failure_attempts: u32` (default 5)
- `max_retry_backoff_ms: u64` (default 300000 = 5min)
- `handoff_states: Vec<String>` (default `["in-review"]`)
- `continuation_delay_ms: u64` (default 1000)

`turn_timeout_ms` already on `RuntimeConfig` (`src/engine/config.rs:138`). Reuse — do not duplicate.

Extend `orchestration_*` tests w/ defaults + override round-trip.

### 2. Event reader per running agent
**ACs**: AC8
**File**: `src/engine/tick.rs` (extend `RunningAgent` from Iter A)

Iter A holds `AgentHandle.events` undrained. Add per-agent reader thread:

```rust
pub struct AgentObservation {
    pub last_event_at: Instant,
    pub tool_use_in_flight: bool,
    pub turn_started_at: Instant,
    pub attempt: u32,
    pub failure_attempt: u32,
    pub exit: Option<Option<i32>>,  // None=running, Some(code)=exited
}

pub struct RunningAgent {
    // existing fields from Iter A
    pub observation: Arc<Mutex<AgentObservation>>,
    pub reader_handle: JoinHandle<()>,
}
```

Reader loop: `recv()` on `handle.events`. On any event: bump `last_event_at`. On `ToolCallStarted`: `tool_use_in_flight = true`. On terminal `ToolCall`: clear. On `TurnCompleted`: reset `turn_started_at`, clear `tool_use_in_flight`. On `SubprocessExited { code }`: record exit, exit loop.

**Current `AgentEvent`** (`src/engine/runner.rs:32`) lacks a tool-start signal. Add variant:

```rust
AgentEvent::ToolCallStarted { name: String }
```

Update `src/engine/runner/claudep.rs` stream parser to emit on stream-json `tool_use` line; existing `ToolCall` stays on `tool_result`. Iter A's exhaustive-match test (`runner.rs:64+`) must be updated.

### 3. Stall detection w/ tool_use suspension
**ACs**: AC8
**File**: `src/engine/tick.rs`

In `run_once` reconcile pass (before dispatch):

```rust
for (doc_id, agent) in running.iter() {
    let obs = agent.observation.lock();
    if obs.tool_use_in_flight { continue; }
    let idle = clock.now_instant().duration_since(obs.last_event_at);
    if idle >= stall_timeout {
        kill_agent_for_retry(agent, RetryReason::Stall);
    }
}
```

`kill_agent_for_retry` = `handle.cancel.send(())`, join reader, release lease, drop from `running`, enqueue retry (task 6).

### 4. Hard turn timeout
**ACs**: AC9
**File**: `src/engine/tick.rs`

Per running agent, independent of tool_use:

```rust
let turn_elapsed = clock.now_instant().duration_since(obs.turn_started_at);
if turn_elapsed >= turn_timeout {
    kill_agent_for_retry(agent, RetryReason::TurnTimeout);
}
```

`turn_timeout_ms` from `config.orchestration.runtime.turn_timeout_ms`. NOT suspended by tool_use (RFC-041: hard wall).

### 5. Doc-status reconcile
**ACs**: AC10, AC11
**File**: `src/engine/tick.rs`

Per running agent per tick, re-load doc via `Store::load` slice:

```rust
let status = current_status_of(&doc_id);
if !active_statuses.contains(&status) {
    if handoff_states.contains(&status) {
        // AC11: handoff — kill, release, KEEP worktree
        kill_agent(agent);
        lease_engine.release(...);
    } else {
        // AC10: terminal — kill, release, REMOVE worktree
        kill_agent(agent);
        lease_engine.release(...);
        workspace::remove(&workspace_path)?;
    }
    running.remove(&doc_id);
}
```

Verify `workspace::remove` exists in `src/engine/workspace.rs` (slice 3). If absent: add `pub fn remove(workspace: &Path) -> Result<()>` calling `git worktree remove --force <path>`.

### 6. Retry queue: continuation + failure backoff
**ACs**: AC12, AC13, AC14
**File**: `src/engine/tick.rs`

```rust
pub struct PendingRetry {
    pub doc_id: String,
    pub doc_type: String,
    pub workspace: PathBuf,
    pub agent_ident: String,
    pub session_id: String,
    pub attempt: u32,
    pub failure_attempt: u32,
    pub ready_at: Instant,
    pub kind: RetryReason,
}

pub enum RetryReason { CleanExit, Stall, TurnTimeout, AbnormalExit, HookFailure }
```

On exit (reader sets `obs.exit = Some(code)`, reconcile observes):

**Clean exit** (AC12): `code == Some(0)`. Doc still active, `attempt < max_turns`:
- `ready_at = now + continuation_delay_ms`. `attempt += 1`. `failure_attempt` unchanged.
- If `attempt == max_turns`: release lease, emit `failed` (reason `max_turns`), no retry.

**Failure** (AC13): `Stall | TurnTimeout | AbnormalExit | HookFailure`. `n = failure_attempt + 1`:
- `delay = min(10000 * 2u64.pow(n-1), max_retry_backoff_ms)`.
- `ready_at = now + delay`. `failure_attempt = n`. `attempt` unchanged.
- If `n > max_failure_attempts` (AC14): release, emit `failed` (reason `max_failure_attempts`), no retry.

Before fresh-dispatch pass each tick: drain `retry_queue` entries where `ready_at <= now`. For each:
- Re-acquire lease against same `doc_id` w/ same `agent_ident` (CAS-against-zeros).
- CAS fail: emit `failed`, abandon.
- CAS ok: re-spawn via `runner.spawn` in same workspace (branch reuse from slice 3). Insert into `running` w/ fresh `turn_started_at`, carried `attempt`/`failure_attempt`.

### 7. Post-cap: release + `failed` event emit
**ACs**: AC14
**File**: `src/engine/tick.rs`

Daemon emits `failed` event, does NOT mutate doc status. Slice 6 owns IPC; sink trait lives here:

```rust
pub trait AgentEventSink: Send + Sync {
    fn emit_failed(&self, doc_id: &str, agent_ident: &str, reason: &str);
}

pub struct NullEventSink;
```

`TickLoop` takes `Box<dyn AgentEventSink>`. Slice 6 wires real IPC sink. Two concrete uses (Null + future IPC) → trait justified per dictum 6.

Failure cap: `emit_failed(doc_id, ident, "max_failure_attempts")`, release lease, drop from running. Workspace left in place (RFC-041 conservative posture).

Continuation cap: same w/ reason `max_turns`.

### 8. `clippy` + `fmt`

`cargo clippy --all-targets --all-features -- -D warnings`.

## Test Plan

### Automated tests

**AC8 — Stall detection w/ tool_use suspension**
- `tick::tests::stall_kills_agent_when_idle_exceeds_timeout` — fake clock advances `stall_timeout_ms`, no events; `handle.cancel` sent.
- `tick::tests::stall_suspended_during_tool_use` — `ToolCallStarted` emitted, clock advances past timeout, no terminal; agent NOT killed.
- `tick::tests::stall_resumes_after_tool_result` — `ToolCallStarted` then terminal `ToolCall`; clock advances past timeout; killed.
- `tick::tests::stall_classified_as_failure` — retry has `kind == Stall`, increments `failure_attempt` only.

**AC9 — Turn timeout**
- `tick::tests::turn_timeout_kills_agent_independent_of_tool_use` — `ToolCallStarted` active, clock past `turn_timeout_ms`; killed.
- `tick::tests::turn_timeout_classified_as_failure` — `kind == TurnTimeout`, increments `failure_attempt`.

**AC10 — Terminal status reconcile**
- `tick::tests::terminal_status_kills_releases_and_removes_workspace` — fake `Store` returns `done`; cancel + release + `workspace::remove` called.
- `tick::tests::terminal_status_does_not_enqueue_retry` — retry queue empty.

**AC11 — Handoff status reconcile**
- `tick::tests::handoff_status_kills_and_releases_but_keeps_workspace` — fake `Store` returns `in-review`; cancel + release; `workspace::remove` NOT called.

**AC12 — Clean exit continuation**
- `tick::tests::clean_exit_enqueues_continuation_with_delay` — `SubprocessExited{Some(0)}`, doc active, `attempt=1`; queue entry `ready_at == now + continuation_delay_ms`, `attempt=2`.
- `tick::tests::continuation_reuses_same_workspace` — re-spawn uses original `workspace` path.
- `tick::tests::continuation_caps_at_max_turns` — `attempt == max_turns`: NO retry, `failed` w/ reason `max_turns`, lease released.
- `tick::tests::continuation_does_not_increment_failure_attempt` — `failure_attempt` unchanged across clean exits.

**AC13 — Failure backoff**
- `tick::tests::failure_backoff_exponential_capped` — table-driven: `n=1→10s, n=2→20s, n=3→40s, n=4→80s, n=5→160s`, w/ cap at `max_retry_backoff_ms=60000` → `n=4→60s`.
- `tick::tests::failure_increments_failure_attempt_only` — `attempt` unchanged.
- `tick::tests::stall_and_abnormal_share_counter` — interleave; one `failure_attempt` counter advances both.

**AC14 — Post-cap**
- `tick::tests::post_cap_releases_lease` — `failure_attempt > max`; `lease_engine.release` called.
- `tick::tests::post_cap_emits_failed_event` — fake `AgentEventSink` records `emit_failed`.
- `tick::tests::post_cap_does_not_mutate_doc_status` — fake `Store` records no write.
- `tick::tests::post_cap_does_not_enqueue_retry` — retry queue empty.

### Manual test plan

Shared setup from ITERATION-176 MT block. Add:

```toml
[orchestration]
stall_timeout_ms = 8000
max_turns = 3
max_failure_attempts = 2
max_retry_backoff_ms = 40000
handoff_states = ["in-review"]
continuation_delay_ms = 1000

[orchestration.runtime]
turn_timeout_ms = 20000
```

Use a fake claude shim `/tmp/fake-claude.sh`:

```bash
#!/usr/bin/env bash
case "$1" in
  --quiet) sleep 600 ;;
  --tool)  printf '{"type":"tool_use","name":"Bash"}\n'; sleep 600 ;;
  --burst) for i in 1 2 3; do printf '{"type":"text","delta":"hi"}\n'; sleep 1; done; sleep 600 ;;
  --clean) printf '{"type":"turn_complete","input_tokens":10,"output_tokens":5}\n'; exit 0 ;;
  --crash) sleep 2; exit 9 ;;
esac
```
`chmod +x`. Point `claude_binary` at it w/ argument suffix per test.

**MT1 — AC8 stall detection**
1. `claude_binary = "/tmp/fake-claude.sh --quiet"`. `stall_timeout_ms = 8000`.
2. Create + assign STORY-X. Start daemon.
3. Expected: ~T+8s after spawn, agent SIGTERMed. Log "stall detected". `pgrep -f fake-claude` empty. Retry queued.

**MT2 — AC8 tool_use suspension**
1. `claude_binary = "/tmp/fake-claude.sh --tool"`. Same stall timeout.
2. Dispatch. Fake prints `tool_use`, no `tool_result`.
3. Run 30s.
4. Expected: agent NOT killed. Log "tool_use in-flight, stall suspended".
5. SIGTERM fake-claude manually. Daemon classifies as abnormal exit → backoff retry.

**MT3 — AC9 turn timeout**
1. `turn_timeout_ms = 20000`. `claude_binary = "/tmp/fake-claude.sh --tool"`.
2. Dispatch. Tool_use in-flight.
3. Expected: at ~T+20s, daemon kills (turn timeout, NOT stall — tool_use suspends stall but not turn). Log "turn timeout". Retry queued as failure.

**MT4 — AC10 terminal status reconcile**
1. `claude_binary = "/tmp/fake-claude.sh --burst"`. Dispatch STORY-X.
2. Confirm worktree at `.lazyspec/work/agents/STORY-X`.
3. Second shell: edit STORY-X `status: done`. Push if remote-backed.
4. Within one poll interval:
5. Expected: agent SIGTERMed, remote lease gone, worktree dir removed, `git worktree list` no longer shows it.

**MT5 — AC11 handoff status reconcile**
1. New story STORY-Y. Dispatch.
2. Edit STORY-Y `status: in-review`.
3. Expected: agent killed, lease released. Worktree dir STILL PRESENT. `git worktree list` shows it. Branch ref intact.

**MT6 — AC12 clean exit continuation**
1. `claude_binary = "/tmp/fake-claude.sh --clean"`. `max_turns = 3`, `continuation_delay_ms = 1000`.
2. Dispatch STORY-X.
3. Timeline expected:
   - T+0: spawn. Exit 0 ~ immediate.
   - T+1s: re-spawn (attempt=2). Exit 0.
   - T+2s: re-spawn (attempt=3). Exit 0.
   - T+3s: attempt 4 > max_turns. NO re-spawn. `failed` event reason `max_turns`. Lease released.
4. `git ls-remote origin refs/lazyspec/leases/story/STORY-X` → empty. Doc status unchanged.

**MT7 — AC13 failure backoff**
1. `claude_binary = "/tmp/fake-claude.sh --crash"`. `max_failure_attempts = 2`, `max_retry_backoff_ms = 40000`.
2. Dispatch. Note timestamps in log.
3. Timeline expected:
   - T+0: spawn. T+2s: crash.
   - Delay `min(10000*2^0, 40000) = 10000`.
   - T+12s: re-spawn (failure_attempt=1). T+14s: crash.
   - Delay `min(10000*2^1, 40000) = 20000`.
   - T+34s: re-spawn (failure_attempt=2). T+36s: crash.
   - failure_attempt would be 3 > max. NO retry. `failed` reason `max_failure_attempts`.

**MT8 — AC14 post-cap**
1. Continuation cap (continue MT6): remote lease empty, doc status unchanged, log shows `failed` w/ reason `max_turns`.
2. Failure cap (continue MT7): same assertions, reason `max_failure_attempts`.

**MT9 — Reconcile + retry interleave**
1. STORY-A (`--clean`), STORY-B (`--crash`). `max_concurrent_agents = 2`. `max_turns = 2`, `max_failure_attempts = 2`.
2. Dispatch both.
3. Expected: A continues to cap (reason `max_turns`). B backs off to cap (reason `max_failure_attempts`). Both `failed` events emitted. Workspaces left in place.

## Notes

- Daemon NEVER mutates doc status (RFC-041 invariant). Reconcile only reacts to status set elsewhere.
- Continuation counter (`attempt`/`max_turns`) and failure counter (`failure_attempt`/`max_failure_attempts`) are DISTINCT. Do not share.
- Stall suspended during tool_use; turn timeout NOT suspended. Hard wall.
- New `AgentEvent::ToolCallStarted` variant; Iter A exhaustive matches must update.
- `AgentEventSink` trait + `NullEventSink` default. Slice 6 (STORY-122) wires real IPC sink.
- Post-cap workspace left in place per RFC-041 conservative posture (mirrors §Boot orphan recovery in Iter C).
- Per-agent reader thread is the new event-consumption seam. Joins on cancel.
