---
title: Agent metadata schema and commit-chain persistence
type: iteration
status: accepted
author: agent
date: 2026-05-14
tags: []
related:
- implements: STORY-124
---

## In Scope

Group A of STORY-124. ACs:

- AC1 commit-chain persist `refs/lazyspec/agents/{sid}` (append, no overwrite)
- AC2 `AgentMetadata` all fields round-trip
- AC3 `crashed` status write by boot recov; prior hist preserved (chain, not orphan)
- AC6 session-start iter-id snapshot in ref data, survives daemon restart
- AC8 read API callable w/o daemon (free fn over `&impl GitRefOps`)

## Out of Scope

Group B sibling iter owns:

- AC4 `metadata_push_interval_ms` push cadence
- AC5 cross-machine fetch+read
- AC7 remote-unreachable tolerance

No remote push call in this iter. Write path local-only + CAS-safe. Trait shape must permit Group B to layer push w/o refactor (i.e. writer impl holds `git: G` only; push wired in Group B as separate concern, e.g. tick-loop calls `push_ref` or new writer method added then).

## Test Plan

Unit tests in `src/engine/agent_metadata.rs` `#[cfg(test)] mod tests`. Fake `GitRefOps` (extend existing `RecordingGit`) records call order + holds in-mem ref state for chain verification. Fixed `DateTime<Utc>` literals, no `Utc::now()`. Integration test in `tests/agent_metadata.rs` (new) against real temp git repo via existing test helpers, covers AC1+AC8 end-to-end.

- AC1 (unit): write metadata twice for same sid → `RecordingGit` records 2 `create_commit` calls, 2nd has `parent = Some(first_sha)`, then 2 `update_ref` calls w/ correct CAS old/new shas. No `create_ref_commit` (orphan) calls. Tradeoff: fake records call shape; integration test confirms real chain.
- AC1 (integration): real git repo, write twice, `git log refs/lazyspec/agents/sid` shows 2 commits, parent linked.
- AC2 (unit): build `AgentMetadata` w/ every field populated (fixed timestamps, all enum variants of `AgentStatus` in separate cases incl `running`/`crashed`), write via fake, capture serialized blob, deserialize, assert field equality each.
- AC3 (unit): pre-seed fake w/ a `running` metadata sha at ref, call `mark_crashed(sid)`, assert (a) new commit has `parent = running_sha`, (b) new blob has `status = crashed`, (c) prior sha still reachable as parent.
- AC3 (integration): real repo, write running then mark_crashed, `git log` shows 2 commits, oldest is running.
- AC6 (unit): write metadata w/ `session_start_iteration_ids = vec!["ITERATION-100","ITERATION-101"]`, read back via `read_agent_metadata`, assert equal. Second write preserves snapshot field (carried in struct, written on every commit).
- AC8 (unit): no daemon constructed; call free fn `read_agent_metadata(&git, root, sid)` against fake w/ seeded ref → returns latest `AgentMetadata`. No mut state, no daemon handle required.
- AC8 (integration): real repo, write metadata, separate read-only call resolves+returns it; smoke that read needs only `GitRefOps`.

## Changes

### 1. Define `AgentMetadata` schema + `AgentStatus` enum (AC2, AC3, AC6)

File: `src/engine/agent_metadata.rs` (existing). Add:

- `pub enum AgentStatus { Running, Idle, Crashed, Completed, ... }` — confirm variant set by scanning callers; minimum `Running` + `Crashed` for current callers, others added as enum-exhaustive variants visible in story body. `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]`, `#[serde(rename_all = "snake_case")]`.
- `pub struct AgentMetadata` w/ fields per RFC-041 sketch + `pub session_start_iteration_ids: Vec<String>` (naming: matches existing `prior_iterations` semantics; confirm via grep before final). `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]`.
- Timestamps `chrono::DateTime<Utc>`.

Verify: `cargo build`, `cargo test -p lazyspec engine::agent_metadata`.

### 2. Extend `AgentMetadataWriter` trait + `GitRefAgentMetadata` impl w/ chained writes (AC1, AC3)

File: `src/engine/agent_metadata.rs`.

- Add `fn write(&self, metadata: &AgentMetadata) -> Result<String>` to trait — returns new chain-head sha. Group B uses this directly to avoid extra `resolve_ref` per push. Impl:
  - Resolve current `refs/lazyspec/agents/{sid}` via `git.resolve_ref` → `Option<String>` prev_sha.
  - Serialize metadata → `metadata.json`.
  - `git.create_commit(root, refname, &[("metadata.json", &json)], prev_sha.as_deref())` → new_sha.
  - `git.update_ref(root, refname, &new_sha, prev_sha.as_deref().unwrap_or(""))` — match lease.rs CAS pattern; for first write (no prev) use zero-sha sentinel per existing convention (verify against `GitCli::update_ref` impl in `git_ref.rs:192`).
  - Return `Ok(new_sha)`. No `push_ref*` — Group B owns push.
- Rewrite `mark_crashed`: read prev sha, read prev blob if exists, build `AgentMetadata` w/ `status = Crashed` (preserve other fields from prev if present; else minimal record w/ sid + crashed + now), call `write(...)`. Signature stays `fn mark_crashed(&self, session_id: &str) -> Result<()>` — boot.rs caller unchanged.
- Remove use of `create_ref_commit` (orphan).
- `NullAgentMetadata`: add no-op `write` returning empty string sha.

`mark_crashed` keeps `Result<()>` (sha discarded internally) — boot.rs caller untouched.

Verify: `cargo test engine::agent_metadata`, `cargo clippy --all-targets -- -D warnings`.

### 3. Free-function read API (AC8)

File: `src/engine/agent_metadata.rs`.

- `pub fn read_agent_metadata<G: GitRefOps>(git: &G, root: &Path, session_id: &str) -> Result<Option<AgentMetadata>>`:
  - `git.resolve_ref` → if `None` return `Ok(None)`.
  - `git.read_ref_blob(root, &sha, "metadata.json")` → parse → `Ok(Some(...))`.
- No daemon, no writer instance required. Re-export from `src/engine/mod.rs`.

Verify: `cargo test engine::agent_metadata::tests::read`.

### 4. Update fake `RecordingGit` in tests (supporting)

File: `src/engine/agent_metadata.rs` (test module).

- Extend `RecordingGit` to implement `create_commit` (record + return synthetic sha tied to parent), `resolve_ref` (return last recorded sha for refname), `read_ref_blob` (return last written blob for sha), `update_ref` (record CAS pair). Drop `create_ref_commit` from production code path; fake may keep no-op.
- Add helpers `RecordingGit::seed(sid, AgentMetadata)` for AC3 prev-state test.

Verify: `cargo test engine::agent_metadata`.

### 5. Boot recovery caller sanity check (AC3)

File: `src/engine/boot.rs` (line 151 area).

- No signature change expected. Re-run boot tests to confirm `metadata.mark_crashed(session_id)` still compiles + behaves (now chains rather than orphans).
- If `RealBootRecovery` test setup uses a fake `AgentMetadataWriter`, ensure it still implements the new `write` method (add no-op).

Verify: `cargo test engine::boot`.

### 6. Daemon constructor compat (supporting)

File: `src/engine/daemon.rs:172`.

- `GitRefAgentMetadata::new(root, GitCli)` signature unchanged. Confirm builds. No tick-loop wiring in this iter.

Verify: `cargo build`, `cargo test daemon`.

### 7. Integration tests (AC1, AC3, AC8)

File: `tests/agent_metadata.rs` (new).

- Use existing temp-repo test helper pattern (grep `tempfile` + `init_repo` in `tests/`).
- Test: write metadata twice → real `git log` shows 2-commit chain.
- Test: write running → mark_crashed → log shows chain, latest blob is `crashed`.
- Test: write → call `read_agent_metadata` w/ a fresh `GitCli` instance (no daemon) → returns latest.

Verify: `cargo test --test agent_metadata`, `cargo clippy --all-targets -- -D warnings`.

## Notes

- **Snapshot field naming:** `session_start_iteration_ids: Vec<String>` chosen to mirror `prior_iterations` terminology in RFC-041 / STORY-125 prose. Grep existing code before finalising; rename if `prior_iterations_snapshot` or similar already exists.
- **Chained `mark_crashed`:** signature preserved (`&str` only), but impl now reads prev sha + prev blob to preserve fields. If prev blob absent (no metadata ever written for that sid — shouldn't happen in real boot path), write a minimal `AgentMetadata { session_id, status: Crashed, started_at: now, last_event_at: now, ..Default::default() }`. Document this fallback in code comment (warranted: non-obvious from sig).
- **boot.rs compat:** signature unchanged; `BootRecovery` trait + `RealBootRecovery` test fakes need a no-op `write` added. No production caller refactor.
- **No remote push in Group A:** Group B sibling iter adds push (likely tick-loop driven w/ interval config). Writer trait kept narrow (`write`, `mark_crashed`) — Group B can add `push_for_session` or call `push_ref_with_lease` directly via `GitRefOps` (preferred — mirrors lease.rs heartbeat). Writer struct already exposes `git: G`, so push wires in w/o trait refactor.
- **CAS first-write sentinel:** check `GitCli::update_ref` in `git_ref.rs:192` for zero-sha convention; lease.rs heartbeat always has a prev, but agent metadata's first write does not. May need a thin branch (create_commit w/ no parent, then plain set-ref instead of CAS update_ref). Verify behaviour during impl.
- **Fake vs real git in tests:** unit tests w/ fake `GitRefOps` are fast + cover call shape; integration tests w/ real git confirm CAS + chain semantics actually work. Both needed — fake alone would mask `update_ref` CAS bugs.
- **No new trait for read path** (Dictum 6): free fn over `&impl GitRefOps` is sufficient for AC8.
