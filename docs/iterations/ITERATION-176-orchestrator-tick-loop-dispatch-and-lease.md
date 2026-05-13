---
title: Orchestrator tick loop dispatch and lease
type: iteration
status: accepted
author: agent
date: 2026-05-13
tags: []
related:
- implements: STORY-128
- blocks: ITERATION-177
---

## Scope

Iter A of STORY-128. AC1-7 only. Reconcile/retry (AC8-14) + boot/preflight (AC15-16) = later iters.

In: poll cadence, eligibility filter, concurrency cap, RFC-035 lease acquire w/ CAS, daemon-side heartbeat, `{host}:{session_id}` agent ident, batched lease fetch piggybacked on metadata-push interval.

Out: stall, turn timeout, status reconcile, continuation, failure backoff, post-cap, orphan recovery, preflight, agent-event consumption (just dispatch + lease — agent lifecycle observability lands in Iter B).

## Changes

### 1. Extend `OrchestrationConfig` w/ tick-loop fields
**ACs**: AC1, AC3, AC5, AC7
**File**: `src/engine/config.rs:172`

Add fields to `OrchestrationConfig`:

- `poll_interval_ms: u64` (default 30000)
- `max_concurrent_agents: usize` (default 4)
- `active_statuses: Vec<String>` (default `["todo","in-progress"]`)
- `heartbeat_interval_ms: u64` (default 300000 = 5min)
- `metadata_push_interval_ms: u64` (default 30000)

Each w/ `#[serde(default = "default_*")]` + `default_*` fn matching existing pattern. Extend `orchestration_*` config tests (`config.rs:1062+`): assert defaults + explicit override round-trip.

Verify: `cargo test -p lazyspec engine::config::tests::orchestration` green.

### 2. Compose `{host}:{session_id}` lease agent ident
**ACs**: AC6
**Files**: `src/engine/agent.rs`

Add to `src/engine/agent.rs`:

```rust
pub fn lease_agent_id(host: &str, session_id: &str) -> String {
    format!("{host}:{session_id}")
}
```

Session id = fresh `uuid::Uuid::new_v4()` per spawn. Verify `uuid` in `Cargo.toml`; add `uuid = { version = "1", features = ["v4"] }` if absent. Host id sourced from `host_id::host_id(root)` (`src/engine/host_id.rs:18`) — `.lazyspec/daemon-host-id` persistence already wired.

Unit test: `lease_agent_id("hostA", "sess-1") == "hostA:sess-1"`. Persistence already covered in existing `host_id` tests.

### 3. `Dispatcher` — pure candidate selection
**ACs**: AC2, AC3
**File**: new `src/engine/dispatcher.rs` + register in `src/engine.rs`

```rust
pub struct Candidate {
    pub doc_id: String,
    pub doc_type: String,
    pub status: String,
    pub priority: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub assignees: Vec<String>,
}

pub struct Dispatcher<'a> {
    pub orchestration: &'a OrchestrationConfig,
    pub active_lease_ids: &'a HashSet<String>,
    pub running_ids: &'a HashSet<String>,
}

impl<'a> Dispatcher<'a> {
    pub fn eligible(&self, candidates: &[Candidate]) -> Vec<Candidate>;
    pub fn slots_available(&self, running_count: usize) -> usize {
        self.orchestration.max_concurrent_agents.saturating_sub(running_count)
    }
}
```

Eligibility filter (AC2): `active_statuses.contains(&c.status) && c.assignees.iter().any(|a| agent_users.contains(a)) && !active_lease_ids.contains(&c.doc_id) && !running_ids.contains(&c.doc_id)`.

Sort (AC2 tie-break): `priority asc, created_at asc, doc_id asc`.

Concurrency cap (AC3): caller takes `eligible.into_iter().take(slots_available)`.

Pure fn — no I/O. Concrete struct, no trait. Dictum 6.

### 4. `TickLoop` — owns timing + candidate fetch + dispatch
**ACs**: AC1, AC2, AC3, AC4, AC5, AC7
**File**: new `src/engine/tick.rs` + register in `src/engine.rs`

```rust
pub trait Clock: Send + Sync {
    fn now_instant(&self) -> Instant;
    fn now_utc(&self) -> DateTime<Utc>;
    fn sleep(&self, dur: Duration);
}

pub struct SystemClock;

pub struct TickLoop<R: AgentRunner + Send, G: GitRefOps + Clone + Send, C: Clock> {
    pub root: PathBuf,
    pub config: Config,
    pub host_id: String,
    pub runner: R,
    pub lease_engine: LeaseEngine<G>,
    pub clock: C,
    pub running: HashMap<String, RunningAgent>,
    pub last_metadata_push: Option<Instant>,
}

pub struct RunningAgent {
    pub session_id: String,
    pub doc_id: String,
    pub doc_type: String,
    pub agent_ident: String,
    pub handle: AgentHandle,
    pub last_heartbeat: Instant,
}
```

`run_once()`:

1. **Metadata-push window check** (AC7): if `last_metadata_push.is_none()` OR elapsed ≥ `metadata_push_interval_ms`, iterate `config.documents.types`, call `fetch_ref_optional(&git, &root, &remote, &lease_glob(&type.name))` per type. Set `last_metadata_push = Some(now)`. Otherwise skip fetch this tick. **Per-tick code never calls `git fetch` outside this gate.**
2. **Reap dead agents**: drain `running` map of agents whose handle subprocess has exited (`waitpid` non-blocking on stored pid OR check cancel channel). Release their leases. Iter A: dead = exited (any reason). Iter B refines.
3. **Heartbeat sweep** (AC5): for each surviving running agent w/ `now.duration_since(last_heartbeat) >= heartbeat_interval_ms`, call `lease_engine.heartbeat(&root, &doc_type, &doc_id, &agent_ident, clock.now_utc())`. Update `last_heartbeat`. Failure logs + drops agent + releases lease.
4. **Fetch candidates** (AC2): `Store::load(&root, &config)`, slice by `orchestration.claim_type`, build `Vec<Candidate>` from doc frontmatter.
5. **Build active_lease_ids**: enumerate local refs via new `GitRefOps::list_refs(root, lease_glob(claim_type))`. No fetch. Parse refname suffix → doc_id → `HashSet`.
6. **Dispatcher pass** (AC2, AC3): `Dispatcher::eligible(...)`, then `take(slots_available(running.len()))`.
7. **Acquire + spawn** (AC4): for each selected candidate:
   - `session_id = Uuid::new_v4().to_string()`, `agent_ident = lease_agent_id(&host_id, &session_id)`.
   - `lease_engine.acquire(&root, &doc_type, &doc_id, &agent_ident, clock.now_utc())`. Existing `acquire` does fetch + CAS-against-zeros via `push_ref_with_lease(..., Some(ZERO_SHA))` (`src/engine/lease.rs:80-95`). CAS failure → log + skip (no spawn — AC4).
   - On lease success: build workspace via existing slice-3 `workspace::ensure_workspace(...)`. Call `runner.spawn(AgentContext { workspace, doc_id, agent_id: agent_ident.clone(), branch })`. Insert into `running` w/ `last_heartbeat = clock.now_instant()`.
   - On non-CAS lease failure: log + skip.
8. **Sleep** `poll_interval_ms` (AC1) via `clock.sleep`.

`run_until(shutdown_rx: Receiver<()>)`: loop `run_once` w/ `recv_timeout(Duration::ZERO)` check between ticks. On shutdown signal: cancel each running agent (`handle.cancel.send(())`), release each held lease via `lease_engine.release`.

**Note**: Iter A holds `AgentHandle.events` but does NOT drain it. Iter B owns event consumption (stall + retry).

### 5. Wire `TickLoop` into `Daemon::run`
**ACs**: AC1, AC5
**File**: `src/engine/daemon.rs:124`

Drain hook at `daemon.rs:145` is the seam. Spawn a tick thread alongside the accept thread when `config.orchestration` is `Some`:

```rust
let tick_shutdown = bounded::<()>(1);
let tick_handle = config.orchestration.as_ref().map(|_| {
    let tl = build_tick_loop(...);
    let rx = tick_shutdown.1.clone();
    thread::spawn(move || tl.run_until(rx))
});
```

Order on shutdown:
1. `running.store(false)` (existing).
2. `tick_shutdown.0.send(())`.
3. Join accept handle.
4. Join tick handle (`TickLoop::run_until` releases its own leases + cancels agents).
5. Existing `release_host_leases` + socket unlink remain — they're a backstop.

Test seam: add `Daemon::with_tick_runner(... Box<dyn TickRunner>)` analogue to existing `with_lease_releaser`. Define minimal `TickRunner: Send`:

```rust
pub trait TickRunner: Send {
    fn run(self: Box<Self>, rx: Receiver<()>) -> Result<()>;
}
```

`TickLoop<R,G,C>` impls `TickRunner`. Tests inject a fake `TickRunner` that records "started" / "shutdown received". Two concrete uses (real + fake) → trait justified per dictum 6.

### 6. List local lease refs (eligibility filter input)
**ACs**: AC2, AC7
**File**: `src/engine/git_ref.rs` (extend `GitRefOps` trait) + `src/engine/lease.rs` (helper)

Add to `GitRefOps`:

```rust
fn list_refs(&self, root: &Path, pattern: &str) -> Result<Vec<String>>;
```

`GitCli` impl: shell `git for-each-ref --format=%(refname) <pattern>`, parse lines. Existing mock `GitRefOps` impls in `lease.rs` tests need a `list_refs` impl too — add a `pub fn local_lease_ids<R: GitRefOps>(git: &R, root: &Path, type_name: &str) -> Result<HashSet<String>>` helper in `lease.rs` that wraps `list_refs` + extracts doc id from refname suffix.

No fetch in this path — local view only.

### 7. Acquire-time fetch — documented as safety net
**ACs**: AC7
**File**: `src/engine/lease.rs:75`

`LeaseEngine::acquire` calls `fetch_ref_optional` unconditionally. RFC-041 §Claim authority designates this as a safety net (stale-local-view recovery). AC7 governs the *eligibility check* fetch, not the acquire-time fetch.

Action: leave the call in place. Add inline comment above `fetch_ref_optional` at `lease.rs:75`:

```rust
// Safety-net fetch: tick-loop eligibility uses local-only reads; acquire-time fetch
// covers the stale-local-view edge case. AC7 (RFC-041) governs eligibility, not acquire.
```

### 8. README + config docs
**ACs**: -
**Files**: `README.md`

CLI flags don't change; config schema does. Document new `[orchestration]` keys in the existing config section:

- `poll_interval_ms`
- `max_concurrent_agents`
- `active_statuses`
- `heartbeat_interval_ms`
- `metadata_push_interval_ms`

Project CLAUDE.md mandates README updates when CLI interface changes — config schema treated as part of that surface.

### 9. `cargo clippy` + `cargo fmt`
**ACs**: -

`cargo clippy --all-targets --all-features -- -D warnings` clean. `cargo fmt --check` clean. Project convention.

## Test Plan

### Automated tests

**AC1 — Polling cadence**
- `tick::tests::run_once_invokes_clock_sleep_with_poll_interval` — fake clock records sleep durations; assert sleep called w/ `Duration::from_millis(poll_interval_ms)`.
- `tick::tests::run_until_fires_n_ticks_in_window` — fake clock advances 60s w/ `poll_interval_ms = 30000`; assert `run_once` invoked twice.

**AC2 — Eligibility filter**
- `dispatcher::tests::eligible_filters_by_status` — candidate w/ status outside `active_statuses` excluded.
- `dispatcher::tests::eligible_requires_agent_assignee_intersection` — candidate w/o assignee in `agent_users` excluded.
- `dispatcher::tests::eligible_excludes_locally_leased_doc` — candidate present in `active_lease_ids` excluded.
- `dispatcher::tests::eligible_excludes_running_doc` — candidate in `running_ids` excluded.
- `dispatcher::tests::eligible_sort_priority_then_created_then_id` — 4 candidates w/ varying values; assert deterministic order.

**AC3 — Concurrency cap**
- `dispatcher::tests::slots_available_respects_max_minus_running` — `max=3, running=2 → 1`.
- `dispatcher::tests::slots_available_saturates_at_zero_when_over_cap` — `max=3, running=5 → 0`.
- `tick::tests::dispatch_takes_at_most_slots_available_candidates` — 5 eligible, `max=3, running=0`, fake runner records spawn count == 3.

**AC4 — CAS acquire before spawn**
- `tick::tests::cas_failure_skips_spawn` — fake `LeaseEngine` returns `Err("CAS rejected")` from `acquire`; assert `runner.spawn` never called for that candidate.
- `tick::tests::lease_acquired_before_spawn` — fake records call order; `acquire` precedes `spawn`.
- `lease::tests::acquire_passes_zero_sha_to_push_with_lease` — existing or new test confirming `push_ref_with_lease(..., Some(ZERO_SHA))`.

**AC5 — Heartbeat cadence**
- `tick::tests::heartbeat_fires_when_interval_elapsed` — running agent w/ `last_heartbeat` advanced by `heartbeat_interval_ms`; fake `LeaseEngine::heartbeat` recorded once.
- `tick::tests::heartbeat_not_fired_before_interval` — agent younger than interval; not called.
- `tick::tests::heartbeat_is_daemon_side` — assert heartbeat agent ident matches stored daemon-side ident, not derived from any `AgentEvent`.

**AC6 — Lease agent identifier shape**
- `agent::tests::lease_agent_id_format` — `lease_agent_id("h","s") == "h:s"`.
- `host_id::tests::host_id_persists_across_calls` — confirm existing test green; add `host_id_creates_uuid_file_when_absent` if not present.
- `tick::tests::dispatch_uses_host_colon_session_for_lease_agent` — fake `LeaseEngine::acquire` records `agent` arg; assert matches regex `^<persisted-host>:[0-9a-f-]{36}$`.

**AC7 — Batched lease fetch**
- `tick::tests::fetch_not_called_per_tick_within_window` — fake `GitRefOps` counts `fetch` calls; run 5 ticks w/ poll_interval=1s, metadata_push_interval=10s; fetch count == 1.
- `tick::tests::fetch_called_again_after_metadata_push_interval` — advance fake clock past interval; assert count increments.
- `tick::tests::fetch_covers_all_configured_type_globs` — config w/ types `["story","iteration"]`; fetch called for each glob in the batch tick.
- `tick::tests::eligibility_path_uses_local_only_reads` — eligibility build calls `list_refs` but NOT `fetch`.

### Manual test plan

Run from a fresh sandbox w/ `cargo run -- daemon` unless noted.

**Setup** (shared):
```
cd /tmp && rm -rf ts-tick && mkdir ts-tick && cd ts-tick
git init && git commit --allow-empty -m init
git remote add origin "$(pwd)/.git"   # self-remote for lease push tests
cargo run --manifest-path /Users/jkaloger/thezone/lazyspec/Cargo.toml -- init
```
Append to `.lazyspec/config.toml`:
```toml
[orchestration]
agent_users = ["claude-bot"]
claim_type = "story"
poll_interval_ms = 5000
max_concurrent_agents = 2
active_statuses = ["draft","todo"]
heartbeat_interval_ms = 10000
metadata_push_interval_ms = 15000

[orchestration.runtime]
claude_binary = "/bin/sleep"
allowed_tools = ""

[coordination]
remote = "origin"
lease_duration = "30m"
grace_period = "1h"
```
Stub claude binary `/bin/sleep` produces long-running benign child for lifecycle observation.

**MT1 — AC1 polling cadence**
1. Terminal A: `RUST_LOG=lazyspec::engine::tick=debug cargo run -- daemon`.
2. Watch logs for "tick fired" entries.
3. Expected: tick at T+0, T+5s, T+10s, T+15s (±1s).
4. SIGTERM after 20s. Daemon exits cleanly w/ exit 0.

**MT2 — AC2 eligibility filter**
1. Create stories:
   ```
   lazyspec create story "A" --author me
   lazyspec create story "B" --author me && lazyspec assign STORY-B --user claude-bot
   lazyspec create story "C" --author me && lazyspec assign STORY-C --user claude-bot
   ```
2. Edit STORY-C frontmatter manually: `status: done`.
3. Plant fake local lease ref for STORY-B: `git update-ref refs/lazyspec/leases/story/STORY-B $(git commit-tree -m fake HEAD^{tree})`.
4. Start daemon. Observe log per tick.
5. Expected: zero spawns. STORY-A skipped (no agent assignee). STORY-B skipped (lease ref present). STORY-C skipped (status not in `active_statuses`).
6. Stop daemon. `git update-ref -d refs/lazyspec/leases/story/STORY-B`. Restart daemon.
7. Expected: STORY-B dispatched exactly once (one stub child). STORY-A and STORY-C still skipped.

**MT3 — AC3 concurrency cap**
1. Create 5 stories all assigned to `claude-bot`, all w/ status in `active_statuses`.
2. Confirm `max_concurrent_agents = 2`.
3. Start daemon.
4. Expected: tick 1 spawns exactly 2 stub children (`pgrep -f /bin/sleep | wc -l` == 2). Subsequent ticks spawn 0 while both running.
5. `kill <one-pid>`. Within one tick after waitpid notices exit, daemon dispatches a third candidate. Process count returns to 2.

**MT4 — AC4 lease acquire w/ CAS**
1. Single story `STORY-X`, assigned, status active.
2. Open second clone in `/tmp/ts-tick-clone` w/ `git clone /tmp/ts-tick`.
3. From clone, race a lease push manually before daemon's CAS push:
   ```
   git commit-tree -m lease HEAD^{tree} ...   # build fake lease commit
   git push origin <sha>:refs/lazyspec/leases/story/STORY-X
   ```
4. Start daemon. Tick attempts acquire → `push_ref_with_lease(..., ZERO_SHA)` fails CAS because remote is non-zero.
5. Expected: log line indicates CAS rejection / "lease held". `pgrep -f /bin/sleep` shows ZERO new children. `runner.spawn` never invoked for STORY-X.
6. Manually delete remote lease: `git push origin :refs/lazyspec/leases/story/STORY-X`. Next tick: daemon acquires + spawns.

**MT5 — AC5 heartbeat cadence**
1. Config `heartbeat_interval_ms = 5000`, `lease_duration = "10m"`.
2. Start daemon. Dispatch one agent.
3. From second shell: `while sleep 6; do git log --format=%H%n%s -n 3 refs/lazyspec/leases/story/STORY-X; echo ---; done`.
4. Expected: lease ref advances ~ every 5s w/ new commits. `git show <sha>:lease.json` shows `expires` shifting forward each tick.
5. Verify agent process is NOT touching git: `strace -p $(pgrep -f /bin/sleep) 2>&1 | grep -i git` → empty. Heartbeat is daemon-side.

**MT6 — AC6 lease agent identifier shape**
1. Fresh workspace, no `.lazyspec/daemon-host-id`. Start daemon. Dispatch one agent.
2. `cat .lazyspec/daemon-host-id` → uuid v4. Save as `H`.
3. `git show $(git rev-parse refs/lazyspec/leases/story/STORY-X):lease.json | jq -r .agent`.
4. Expected: `agent` matches `<H>:<uuid-v4>`.
5. SIGTERM, restart daemon. Dispatch a second story STORY-Y.
6. Expected: `cat .lazyspec/daemon-host-id` unchanged (== H). New lease `agent` = `<H>:<different-uuid>`.

**MT7 — AC7 batched lease fetch**
1. Config `poll_interval_ms = 2000`, `metadata_push_interval_ms = 20000`.
2. Empty store (or one story already dispatched + heartbeating) so no acquire work this tick.
3. Launch: `GIT_TRACE=1 cargo run -- daemon 2>&1 | tee /tmp/trace.log`.
4. Run 60s.
5. Filter: `grep -c "fetch.*refs/lazyspec/leases" /tmp/trace.log`.
6. Expected: ~3 batched fetches (one per metadata-push window). NOT ~30 (one per tick).
7. Compare against an acquire-tick: when daemon acquires a fresh lease, an extra fetch shows up — that's the safety-net path, expected.

**MT8 — Clean shutdown integration**
1. Start daemon. Dispatch 2 agents.
2. SIGTERM.
3. Expected:
   - Tick loop exits between ticks (no panic).
   - Both stub children receive cancel → exit.
   - `git ls-remote origin refs/lazyspec/leases/story/*` → empty.
   - `.lazyspec/daemon.sock` removed.
   - Exit code 0.

## Notes

- Iter A defers `AgentEvent` consumption. `AgentHandle.events` is held but not drained. Iter B introduces the event reader thread (needed for stall detection).
- `Clock` trait = new I/O seam per dictum 4. `SystemClock` for prod, `FakeClock` in test module.
- `Dispatcher` = concrete struct, no trait. Single concrete use this iter. Dictum 6.
- `TickRunner` trait at the `Daemon::with_tick_runner` seam. Two concrete uses (real + fake) → trait justified.
- Acquire-time fetch (`lease.rs:75`) retained as safety net per RFC-041 §Claim authority. AC7 governs eligibility-check fetch, not acquire-fetch.
- Heartbeat is daemon-side. Agent process never holds the lease nor heartbeats. Cemented this iter.
- `host_id::host_id(root)` already persists `.lazyspec/daemon-host-id`. AC6 mostly composes existing primitives w/ new `lease_agent_id` formatter.
- `git fetch` batching = per metadata-push interval, not per-tick. 5s tick must not pound the remote.
- No IPC agent-event emission this iter. Slice 6 (STORY-122) owns IPC.
- `RealLeaseReleaser` (slice 2) is the shutdown backstop. `TickLoop::run_until` releases as it tears down — releaser sweeps anything left.
- `WorkspaceProvisioner` trait added in `tick.rs` to keep unit tests off real `git worktree`. Two uses (prod `GitWorktreeProvisioner` + test fake) — justified per dictum 6.
- `LeaseOps` trait introduced in `tick.rs` w/ blanket impl for `LeaseEngine<G>`. Test fakes record acquire/heartbeat/release calls w/o wiring multi-step `MockGitRefClient` sequences. Two concrete uses — justified.

## Iter A Follow-ups (Iter B / later)

- **Status vocab alignment**: default `active_statuses = ["todo","in-progress"]` includes `"todo"` which `Status` enum doesn't emit (`Status` has Draft/Review/Accepted/InProgress/Complete/Rejected/Superseded). Default vec's first entry is dead. Resolve by adding `Status::Todo` or changing default to `["draft","in-progress"]`.
- **Reap robustness**: `tick.rs::reap_exited` uses `try_recv` for `SubprocessExited` only. Silently discards other events and `Disconnected`. If channel disconnects without an Exited event (handle dropped on panic), running entry leaks. Iter B owns event consumption; fix there.
- **`run_until` responsiveness**: `recv_timeout(Duration::ZERO)` + `clock.sleep(poll_interval_ms)` inside `run_once` means shutdown signal during sleep blocks until sleep completes. Acceptable for 30s default poll. Replace w/ `recv_timeout(poll_interval)` in Iter B.
