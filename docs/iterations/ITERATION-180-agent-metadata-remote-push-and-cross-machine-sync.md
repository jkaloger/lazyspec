---
title: Agent metadata remote push and cross-machine sync
type: iteration
status: draft
author: agent
date: 2026-05-14
tags: []
related:
- implements: STORY-124
---

## In Scope

STORY-124 Group B: remote sync layer on top of Group A's local write/read API.

- **AC4**: push `refs/lazyspec/agents/*` to remote on `[orchestration] metadata_push_interval_ms` cadence (default 30000). Changing config changes observed cadence.
- **AC5**: cross-machine visibility — clone B fetches and reads same `AgentMetadata` clone A wrote.
- **AC7**: remote unreachable does not block local writes. Push retries next interval. Accumulated commits drain once remote reachable.

**Dependency**: Group A (sibling iteration) lands `AgentMetadata` struct + serde, `AgentMetadataWriter` trait, `GitRefAgentMetadata` local impl (`create_commit(parent)` + `update_ref(new, old)`, no push), and library read fn. This iteration consumes that surface and adds the push path; if Group A merges first, no API breakage expected.

## Out of Scope

- AC1 (commit chain persistence), AC2 (round-trip), AC3 (crashed status), AC6 (session-start snapshot), AC8 (non-daemon read API) — owned by Group A.
- Windows named-pipe transport — deferred beyond v1 (RFC-041).
- Retention/GC of accumulated session refs — out of scope per STORY-124 Notes.
- Metrics infra. Observability is `eprintln!` log only this slice; counters land when a metrics seam exists.
- Mutation of doc status on push failure — daemon does not mutate doc status (RFC-041 invariant).

## Test Plan

All behavioural, deterministic, no `sleep`. Fake `GitRefOps` records push/fetch calls and is configurable to fail. Time control via injected `Instant` (or tick counter) mirroring `tick.rs` `last_metadata_push` pattern.

- **AC4 cadence**: drive tick loop N times with `metadata_push_interval_ms = 30000`; assert push call count matches `floor(elapsed / interval)`. Repeat with `interval = 5000`; assert cadence changes accordingly. Sentinel: zero pushes before first interval elapses with a pending local write.
- **AC5 cross-machine**: **unit-level fake** recording push+fetch sequence. Writer A writes → push records `(refname, new_sha, expected_old)`; fake remote stores blob. Reader B fetch reads same blob, deserialises to identical `AgentMetadata`. **Tradeoff**: chose unit-fake over two-tempdir-bare-remote integration because (a) RFC-041 dictum 4 puts I/O at trait seam, (b) two-repo integration duplicates what `GitRefOps` impl tests already cover at the seam, (c) deterministic + fast. Risk: real git push semantics not exercised here — acceptable because `push_ref_with_lease` already has impl-level coverage in `git_ref.rs`.
- **AC7 unreachable + retry + drain**: fake configured to return `Err` on push. Drive write → assert local ref state unchanged-as-committed (write succeeded), assert no error propagated to writer caller. Drive next interval → assert push retried. Flip fake to success → assert next interval push carries the chain head (latest sha); one push call drains all accumulated commits (chain is linear, head sha covers parents).
- **Observability**: assert `eprintln` on push failure (capture via existing test pattern or assert on a log-capturing fake). Logged once per failed interval, not per accumulated commit.

## Changes

### Task 1 — Metadata push entry point on `GitRefAgentMetadata`

**ACs**: AC4, AC7.

**Files**:
- `src/engine/agent_metadata.rs` (Group A's new file — extend; if Group A path differs, follow Group A's choice).

**Implementation**:
- Add `push(&self, root: &Path, session_id: &str, remote: &str) -> Result<()>` method on the `GitRefAgentMetadata` impl (not a new trait — dictum 6).
- Reads local chain head via `git.resolve_ref(root, &refname)`. If `None`, no-op (nothing to push).
- CAS expected-old: track last-pushed sha per session in an in-memory map keyed by `session_id`, seeded lazily from `git.resolve_ref(root, &remote_tracking_refname)` on first push for a session. Avoids per-push remote read.
- Calls `git.push_ref_with_lease(root, remote, &refname, new_sha, expected_old)`.
- On success: update the in-memory `last_pushed` map.
- On failure: `eprintln!("metadata push {}: {}", refname, e)`; **do not propagate**. Return `Ok(())`. Next interval retries naturally (chain head moved forward; CAS expected-old stays at last successful push so push covers all accumulated commits).
- `refname` = `refs/lazyspec/agents/{session_id}`.

**Verification**:
- `cargo test -p lazyspec agent_metadata::push` (or equivalent path).
- `cargo clippy --all-targets -- -D warnings`.

### Task 2 — Wire push into tick loop on `metadata_push_interval_ms` gate

**ACs**: AC4, AC7.

**Files**:
- `src/engine/tick.rs` — extend the existing `push_due` block (currently `tick.rs:417-433`).
- `src/engine/daemon.rs:172` — thread the metadata writer (and remote name) into the tick state at daemon construction.

**Implementation**:
- Inside the existing `if push_due { ... }` block in `tick.rs`, after the lease-fetch loop, iterate **active sessions** (from `self.running`) and call `metadata.push(&self.root, session_id, &coord_remote)` for each.
- `self.last_metadata_push = Some(now_instant)` already captures cadence for both lease fetch and metadata push (single gate per RFC-041 §Orchestrator tick loop).
- Errors from `push` are already swallowed inside the method (Task 1); tick caller never sees them.
- Daemon constructs `GitRefAgentMetadata` once and shares with tick state.

**Verification**:
- `cargo test -p lazyspec tick::metadata_push` (new test module).
- Existing tick tests must still pass: `cargo test -p lazyspec tick`.
- `cargo clippy --all-targets -- -D warnings`.

### Task 3 — Cross-machine fetch path

**ACs**: AC5.

**Files**:
- `src/engine/agent_metadata.rs` — add `fetch_all(&self, root: &Path, remote: &str) -> Result<()>` on `GitRefAgentMetadata`.

**Implementation**:
- Uses existing `fetch_ref_optional(&self.git, root, remote, "refs/lazyspec/agents/*")` helper from `src/engine/lease.rs:52` (reuse, do not fork).
- Tolerates "couldn't find remote ref" as non-error (matches lease pattern).
- Called by tick loop in the same `push_due` block (one gate for all metadata sync per RFC-041) — extends Task 2.
- Group A's read API reads local refs only; after fetch, local refs reflect remote state, so the reader serves both same-clone and other-clone writers transparently.

**Verification**:
- `cargo test -p lazyspec agent_metadata::fetch` — fake records `fetch_refs:origin:refs/lazyspec/agents/*` once per interval.
- Round-trip test: writer A → push to fake remote → fetch on clone B → reader returns identical `AgentMetadata`.
- `cargo clippy --all-targets -- -D warnings`.

### Task 4 — Tick-loop test coverage for cadence, unreachable, drain

**ACs**: AC4, AC7.

**Files**:
- `src/engine/tick.rs` (test module) or a dedicated `src/engine/agent_metadata.rs` test module covering tick-integration scenarios.

**Implementation** (test code only; behavioural):
- Build tick state with fake `GitRefOps`, one running session, `metadata_push_interval_ms = 1000` (test value).
- Cadence: advance injected `now_instant` by 500ms → no push call. Advance another 500ms → one push call. Verify call count on the fake. Change interval to 200ms → cadence reflects new value.
- Unreachable: fake returns `Err` for `push_ref_with_lease`. Drive 3 intervals → 3 push attempts, all fail, no error bubbles to tick caller. Local ref state (inspected via fake's recorded `update_ref` from Group A's path) is unchanged from last commit-chain-write.
- Drain: fake flips to success at interval 4 → assert single push with chain-head sha covering all accumulated commits since last success (CAS expected-old = last successful push sha, not all-zeros after first success).

**Verification**:
- `cargo test -p lazyspec tick::metadata_push_cadence tick::metadata_push_unreachable tick::metadata_push_drain`.
- `cargo clippy --all-targets -- -D warnings`.

### Task 5 — Doc hygiene (README only if surface changed)

**ACs**: none directly; documentation hygiene per project CLAUDE.md.

**Files**:
- `README.md` — only if any new user-facing flag/config. Expected: **no change** (`metadata_push_interval_ms` already documented under RFC-041 work).

**Implementation**:
- Verify `metadata_push_interval_ms` is mentioned in README orchestration section. If absent, add one line under config.
- No CLI surface changes this iteration.

**Verification**:
- `cargo run --quiet -- --help` unchanged.
- `cargo clippy --all-targets -- -D warnings` clean.

## Notes

- **Push scheduling**: rides existing `tick.rs:417-433` gate (`metadata_push_interval_ms`). Single timer; both lease fetch and metadata push+fetch drain on the same tick. No separate timer in `daemon.rs`. Matches RFC-041 §Orchestrator tick loop ("batched lease fetch piggybacked on metadata-push interval").
- **CAS expected-old strategy**: per-session in-memory `last_pushed: HashMap<session_id, Sha>`, seeded lazily from `resolve_ref(remote-tracking)` on first push attempt for a session, advanced on successful push. First-ever push for a session: `expected_old = None` (CAS against zero-sha — matches `lease.rs` `push_ref_with_lease` pattern). On push failure the map is **not** advanced, so the next interval's CAS still targets the last successful state and the new push's commit chain naturally covers all accumulated commits.
- **Observability**: `eprintln!` only this slice. Push failures log once per failed interval (not per accumulated commit). Future: metrics counter `metadata_push_failures_total` when a metrics seam exists. Out of scope here per "no premature abstraction".
- **Group A dependency**: this iteration assumes Group A merges first (or concurrently with no API churn). If `AgentMetadataWriter` trait or `GitRefAgentMetadata` struct path changes, Task 1/3 file paths shift accordingly. Open coordination point: prefer Group A's `write(metadata)` returning the new chain-head `Sha` (avoids a second `resolve_ref` to compute CAS). Confirm at integration time.
- **Push-fetch ordering**: in the tick `push_due` block, push **before** fetch within the same gate. Push reflects this clone's authoritative state; fetch then incorporates other clones'. Matches lease pattern.
- **Dictum 3 (layering)**: engine only. No CLI/TUI changes. No `assign`/`daemon` surface touched.
- **Dictum 4 (I/O at trait seams)**: all network I/O via `GitRefOps`. Tests use fake `GitRefOps` exclusively; no real git in this iteration's tests.
- **Dictum 6 (no premature abstraction)**: `push`/`fetch_all` are methods on `GitRefAgentMetadata`, not a new `MetadataPusher` trait. Scheduling lives in the existing tick gate, not a new timer abstraction.
- **DICTUM-004 (testing)**: tests isolated, deterministic, fast, behavioural. Time injected. No `sleep`. Real git surfaces covered by `GitRefOps` impl tests, not duplicated here.
