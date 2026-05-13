---
title: Orchestrator tick loop boot recovery and preflight
type: iteration
status: draft
author: agent
date: 2026-05-13
tags: []
related:
- implements: STORY-128
---

## Scope

Iter C of STORY-128. AC15-16. Depends on Iter A (ITERATION-176) + Iter B (ITERATION-177).

In: boot orphan lease recovery (wait grace_period, admin-release, mark crashed), preflight at daemon start, preflight re-run on notify events against config + prompt files.

Out: agent metadata refs schema + push (slice 8 / STORY-124 — Iter C only marks the `crashed` state, schema lands in slice 8). IPC + status CLI (slice 6). Status mutation by daemon (still forbidden).

## Changes

### 1. Boot orphan lease scan
**ACs**: AC15
**File**: `src/engine/tick.rs` (new `boot_recovery` module or fn) + `src/engine/lease.rs`

On daemon start, before tick loop runs:

```rust
pub fn boot_orphan_recovery<G: GitRefOps>(
    root: &Path,
    host_id: &str,
    lease_engine: &LeaseEngine<G>,
    config: &Config,
    clock: &impl Clock,
    metadata: &impl AgentMetadataWriter,  // see task 4
) -> Result<()> {
    // For each configured doc type:
    //   list local leases matching pattern
    //   for each, read lease.json, parse `agent` field
    //   if agent starts with `{host_id}:`, it's our orphan
    //   wait grace_period (single sleep), then admin_release
    //   leave worktree in place
    //   mark refs/lazyspec/agents/{session-id} status = crashed
}
```

Iterate `config.documents.types`, call `git.list_refs(root, &lease_glob(type))`. For each ref, `git.read_ref_blob(root, sha, "lease.json")` → parse `Lease` → check `lease.agent.starts_with(&format!("{host_id}:"))`.

Build `Vec<OrphanLease>` first. If empty: return immediately.

Otherwise: `clock.sleep(grace_period)` where `grace_period = parse_duration(&config.coordination.grace_period)?`. RFC-041 says reuse RFC-035 grace_period — do not introduce new timer.

After sleep: for each orphan, extract `session_id` from `agent` (split on `:`), call `lease_engine.admin_release(root, type, doc_id, &orphan.agent)`, then `metadata.mark_crashed(&session_id)`.

Worktree NOT touched.

### 2. `AgentMetadataWriter` seam
**ACs**: AC15
**File**: new `src/engine/agent_metadata.rs`

Slice 8 (STORY-124) owns the full `refs/lazyspec/agents/{session-id}` commit-chain schema. Iter C needs only `mark_crashed`. Define minimal trait:

```rust
pub trait AgentMetadataWriter: Send + Sync {
    fn mark_crashed(&self, session_id: &str) -> Result<()>;
}

pub struct GitRefAgentMetadata<G: GitRefOps> { /* root, git */ }

pub struct NullAgentMetadata;  // no-op fallback when slice 8 not landed
```

`GitRefAgentMetadata::mark_crashed` writes a minimal commit to `refs/lazyspec/agents/{session-id}` w/ a `status.txt` blob containing `crashed`. Slice 8 will replace w/ proper schema; the ref name + crashed marker survive.

Two concrete uses (Null + GitRef) → trait justified per dictum 6.

### 3. Preflight at daemon start
**ACs**: AC16
**File**: new `src/engine/preflight.rs`

```rust
pub struct PreflightChecks<'a> {
    pub root: &'a Path,
    pub config: &'a Config,
}

pub struct PreflightReport {
    pub workflow_readable: bool,
    pub prompt_renders: bool,
    pub agent_users_non_empty: bool,
}

impl PreflightReport {
    pub fn is_ok(&self) -> bool {
        self.workflow_readable && self.prompt_renders && self.agent_users_non_empty
    }
}

pub fn run_preflight(checks: &PreflightChecks) -> Result<PreflightReport> { ... }
```

Checks:

1. **Workflow file readable**: `config.orchestration` exists; `fs::metadata(workflow_path)` succeeds. Workflow path = `.lazyspec/workflows/<role>.md` per RFC-041 trajectory note, BUT v1 single-role uses `.lazyspec/prompts/builder.md` (per RFC-041 §Prompt rendering). v1 reads from `config.orchestration.prompt_path` if present, else default `.lazyspec/prompts/builder.md`.
2. **Prompt renders**: load prompt template w/ minijinja, render against a dummy context (`doc = {id:"DUMMY", title:"x", body:"", status:"draft", assignees:[]}, attempt = null, prior_iterations = []`). Strict-undefined mode (already RFC-041). Any missing variable or syntax error fails.
3. **`agent_users` non-empty**: `config.orchestration.agent_users.is_empty() == false`.

Note: prompt rendering itself is slice 5 (STORY-125). Iter C uses a minimal renderer stub if slice 5 hasn't landed. If `.lazyspec/prompts/builder.md` doesn't exist: preflight fails w/ "prompt template missing" — slice 5 will ship the default template.

Preflight failure: daemon logs the failed checks and **does not start dispatching**. Daemon process still runs (accepts socket connections — slice 6 surfaces preflight status). Iter C: dispatch is gated on `last_preflight.is_ok()`.

### 4. Notify-driven preflight re-run
**ACs**: AC16
**File**: `src/engine/tick.rs` + new watcher

Use `notify` crate (verify in `Cargo.toml`; add `notify = "6"` if absent — preferred over polling stat).

```rust
pub struct PreflightWatcher {
    pub config_path: PathBuf,
    pub prompt_path: PathBuf,
    pub rx: Receiver<notify::Event>,
    _watcher: notify::RecommendedWatcher,
}
```

Spawn watcher at daemon start. `TickLoop` polls `watcher.rx.try_recv()` between ticks. On any event for either path: set `preflight_dirty = true`. Next tick: re-run preflight before dispatch. If now-failing: stop new dispatches (in-flight agents continue; they're not yanked).

Per RFC-041: hot-reload applies to future ticks; in-flight sessions are not restarted.

### 5. Wire boot recovery + preflight into `Daemon::run`
**ACs**: AC15, AC16
**File**: `src/engine/daemon.rs`

Sequence on `Daemon::run`:

1. `bind_listener` (existing).
2. `boot_orphan_recovery(...)` — blocks for grace_period if orphans found. Logs progress.
3. `run_preflight(...)` — fail-loud log if any check fails, but daemon continues.
4. Spawn accept thread (existing).
5. Spawn tick thread w/ `TickLoop` carrying preflight watcher + initial preflight report. `TickLoop::run_until` gates dispatch on preflight status.
6. Block on `shutdown_rx`. Shutdown order from Iter A unchanged.

Boot recovery is synchronous (intentional: RFC-041 wants conservative gate before any dispatch). If grace_period = 1h, daemon hangs there for an hour. Document that explicitly in user guide.

Test seam: `Daemon::with_boot_recovery(Box<dyn BootRecovery>)` — single trait method, lets tests inject a no-op. Two concrete uses (real + no-op) → trait justified.

### 6. README + user guide
**ACs**: AC15, AC16
**File**: `README.md` + `docs/` user guide section

Document:
- Boot recovery behaviour: daemon blocks for `grace_period` on startup if it finds orphan leases from this host. Document tuning.
- Preflight checks: list the three checks + their failure modes.
- Notify-driven re-run: config + prompt file changes re-validate; in-flight agents continue.

### 7. `clippy` + `fmt`

`cargo clippy --all-targets --all-features -- -D warnings`.

## Test Plan

### Automated tests

**AC15 — Boot orphan recovery**
- `tick::tests::boot_recovery_finds_orphans_by_host_prefix` — fake `GitRefOps` returns refs w/ `lease.agent` starting w/ `host_id:`; assert classified as orphans. Refs w/ other host prefix NOT classified.
- `tick::tests::boot_recovery_waits_grace_period` — fake clock; assert `clock.sleep(grace_period)` called once when orphans exist. NOT called when no orphans.
- `tick::tests::boot_recovery_admin_releases_each_orphan` — fake `LeaseEngine::admin_release` records calls; one per orphan, w/ matching agent ident.
- `tick::tests::boot_recovery_marks_session_crashed` — fake `AgentMetadataWriter::mark_crashed` records calls; session_id extracted from `agent` via `:` split.
- `tick::tests::boot_recovery_leaves_worktree_in_place` — fake workspace API NOT called for remove.
- `tick::tests::boot_recovery_noop_when_no_orphans` — empty ref list: no sleep, no admin_release, no mark_crashed.
- `tick::tests::boot_recovery_ignores_other_host_leases` — refs w/ `agent` starting w/ `otherhost:` skipped.

**AC16 — Preflight**
- `preflight::tests::workflow_readable_returns_false_when_missing` — config path doesn't exist.
- `preflight::tests::prompt_renders_returns_false_on_template_error` — template w/ undefined variable in strict-undefined mode.
- `preflight::tests::prompt_renders_returns_true_on_clean_template` — valid template.
- `preflight::tests::agent_users_non_empty_returns_false_on_empty_vec`.
- `preflight::tests::is_ok_only_when_all_pass`.
- `tick::tests::preflight_failure_gates_dispatch` — preflight report w/ `is_ok=false`; tick runs but acquires nothing, no spawn.
- `tick::tests::preflight_watcher_marks_dirty_on_config_change` — fake notify event; assert `preflight_dirty == true`.
- `tick::tests::preflight_rerun_after_dirty_flag` — set dirty → next tick calls `run_preflight` → on pass: dispatch resumes.
- `tick::tests::preflight_failure_does_not_yank_in_flight_agents` — agent running, preflight goes false: running agent untouched.

### Manual test plan

Shared setup from ITERATION-176 MT block.

**MT1 — AC15 boot orphan recovery, happy path**
1. Start daemon, dispatch STORY-X. Wait until lease ref exists.
2. `kill -9` daemon process (simulates crash). Verify lease ref STILL present locally: `cat .git/refs/lazyspec/leases/story/STORY-X`. Worktree directory STILL present.
3. Set `coordination.grace_period = "5s"` for the test (default 1h too slow).
4. Restart daemon w/ `RUST_LOG=lazyspec::engine=info`.
5. Expected log timeline:
   - T+0: "boot recovery: found 1 orphan lease (host=<H>)".
   - T+0: "waiting grace_period=5s".
   - T+5s: "admin-releasing orphan STORY-X".
   - T+5s: "marking session <session-id> as crashed".
   - T+5s: tick loop starts.
6. After T+5s verify:
   - `git ls-remote origin refs/lazyspec/leases/story/STORY-X` → empty.
   - Worktree directory `.lazyspec/work/agents/STORY-X` STILL present (`ls` shows it).
   - `git show refs/lazyspec/agents/<session-id>:status.txt` → `crashed`.

**MT2 — AC15 boot recovery filters by host prefix**
1. Manually plant a fake orphan lease w/ `agent = "OTHERHOST:fake-session"`:
   ```
   echo '{"agent":"OTHERHOST:fake","acquired":"2026-01-01T00:00:00Z","expires":"2026-12-31T23:59:59Z"}' > /tmp/lease.json
   ... git commit-tree + update-ref + push to mimic real lease ...
   ```
2. Also plant a real orphan from this host (`<H>:our-session`).
3. Restart daemon. Watch logs.
4. Expected: only the `<H>:our-session` orphan triggers grace_period wait + admin_release. `OTHERHOST:fake` lease untouched. After recovery: only our orphan is gone.

**MT3 — AC15 no orphans → instant boot**
1. Fresh workspace (no leases). Start daemon.
2. Expected: log "boot recovery: 0 orphans". No grace_period sleep. Tick loop starts within ~T+1s.

**MT4 — AC16 preflight at start, happy path**
1. Workspace w/ valid config: `agent_users = ["claude-bot"]`, `.lazyspec/prompts/builder.md` exists w/ valid template.
2. Start daemon. Watch logs.
3. Expected: "preflight: workflow_readable=true prompt_renders=true agent_users_non_empty=true → ok". Tick loop dispatches normally.

**MT5 — AC16 preflight fails when agent_users empty**
1. Edit `.lazyspec/config.toml` → `agent_users = []`.
2. Start daemon.
3. Expected: log "preflight: agent_users_non_empty=false → blocked". Daemon process keeps running (socket bound), tick loop runs but does NOT dispatch.
4. Create + assign a story → not picked up (no dispatch).

**MT6 — AC16 preflight fails when prompt missing**
1. `rm .lazyspec/prompts/builder.md`. Start daemon.
2. Expected: log "preflight: prompt_renders=false → blocked". No dispatch.

**MT7 — AC16 preflight fails when prompt has undefined var**
1. Write `.lazyspec/prompts/builder.md` containing `{{ nonexistent_var }}`. Start daemon.
2. Expected: log indicates minijinja strict-undefined error. Preflight fails.

**MT8 — AC16 notify-driven re-run on config edit**
1. Start daemon w/ valid config. Confirm tick loop dispatching.
2. Edit `.lazyspec/config.toml` → `agent_users = []`.
3. Within seconds of save: expected log "preflight invalidated (config changed)" then "preflight: blocked". No new dispatches.
4. Edit back to `agent_users = ["claude-bot"]`. Save.
5. Expected: "preflight invalidated" → "preflight: ok". New dispatches resume.

**MT9 — AC16 notify-driven re-run on prompt edit**
1. Start daemon. Confirm dispatching.
2. Edit `.lazyspec/prompts/builder.md` → introduce undefined var.
3. Expected within seconds: log "preflight invalidated (prompt changed)" → "preflight: blocked".
4. Revert. Save. → "preflight: ok".

**MT10 — AC16 in-flight agents not yanked on preflight failure**
1. Start daemon, dispatch STORY-X (active agent w/ stub child).
2. Edit config to make preflight fail.
3. Expected: STORY-X agent KEEPS running. Heartbeat continues. Only NEW dispatches blocked. RFC-041: hot-reload applies to future ticks, not in-flight sessions.

**MT11 — Integration: boot recovery + preflight combined**
1. Crash daemon mid-run (kill -9). Confirm orphan lease + worktree intact.
2. Edit config to make preflight fail.
3. Restart daemon.
4. Expected order in log:
   - boot recovery starts.
   - grace_period wait.
   - admin-release orphan.
   - mark crashed.
   - preflight runs → fails.
   - tick loop runs but does NOT dispatch.
5. Fix config. Preflight watcher fires. Daemon resumes dispatching.

## Notes

- Boot recovery is BLOCKING. Daemon hangs for `grace_period` if orphans found. Default RFC-035 grace_period = configurable; document the implication for ops.
- Daemon NEVER mutates doc status during recovery. Only releases leases + marks agent ref crashed.
- `mark_crashed` writes a minimal `status.txt = "crashed"` blob to `refs/lazyspec/agents/{session-id}`. Slice 8 (STORY-124) replaces w/ full `AgentMetadata` schema; the `crashed` marker survives.
- Preflight gates dispatch but does NOT stop the daemon process. Operator can fix config in place; notify watcher revalidates.
- `notify` crate preferred over polling (Rust ecosystem convention per dictum 5).
- In-flight agents survive preflight regression. Hot-reload applies to future ticks only.
- Worktrees left in place after orphan recovery (RFC-041 conservative: lets operator decide resume vs discard).
- Test seams added this iter: `BootRecovery`, `AgentMetadataWriter`. Both have two concrete uses each.
- Iter C closes out STORY-128 acceptance. After this, slices 5-8 of RFC-041 remain (prompts, IPC, TUI agents view, metadata refs schema).
