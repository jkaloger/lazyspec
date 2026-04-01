---
title: Lease engine, CLI subcommands, and write gate
type: iteration
status: accepted
author: agent
date: 2026-04-01
tags: []
related:
- implements: STORY-108
---


## Context

Second of two iterations against STORY-108. Depends on Iteration A (GitRefOps trait, config, agent identity). Delivers the lease engine, CLI subcommands for lease management, and lease-gate enforcement on writes.

## Acceptance Criteria Addressed

From STORY-108:
- AC Group 2: Lease acquire/release/heartbeat/force-acquire/query (all 9 criteria)
- AC Group 4: CLI subcommands with --json (all 5 criteria)
- AC Group 5: Lease-gate enforcement on writes (all 3 criteria)

## Changes

1. **Create `src/engine/lease.rs` — Lease engine**
   - ACs: Group 2 (all 9 criteria)
   - Define `Lease` struct: `agent: String`, `acquired: DateTime<Utc>`, `expires: DateTime<Utc>` (serialized as `lease.json`)
   - Define `LeaseEngine<R: GitRefOps>` struct holding a `GitRefOps` implementation and `CoordinationConfig`
   - Implement `acquire(&self, root: &Path, type_name: &str, id: &str, agent: &str) -> Result<Lease>`:
     - Create `lease.json` blob via `GitRefOps::create_ref_commit` on `refs/lazyspec/leases/{type}/{id}`
     - Push to remote. If ref already exists on remote, fail with "lease held" error
   - Implement `release(&self, root: &Path, type_name: &str, id: &str, agent: &str) -> Result<()>`:
     - Resolve ref, read lease.json, verify caller is holder
     - Delete remote ref via `GitRefOps::delete_remote_ref`, then delete local ref
   - Implement `admin_release(&self, root: &Path, type_name: &str, id: &str, expected_holder: &str) -> Result<()>`:
     - Same as release but takes `expected_holder` parameter, verifies it matches, bypasses expiry checks
   - Implement `heartbeat(&self, root: &Path, type_name: &str, id: &str, agent: &str) -> Result<Lease>`:
     - Read current lease, verify caller is holder
     - Create new commit with updated expiry, parented on current lease commit
     - `update_ref` with CAS (old SHA), then push
   - Implement `force_acquire(&self, root: &Path, type_name: &str, id: &str, agent: &str) -> Result<Lease>`:
     - Read current lease, check `now > expires + grace_period`
     - If expired beyond grace: delete old lease, acquire new one
     - If still within grace period: fail
   - Implement `query(&self, root: &Path) -> Result<Vec<(String, Lease)>>`:
     - `list_refs` on `refs/lazyspec/leases/*`, read each lease.json, return list
   - Register module in `src/engine/mod.rs`
   - Verification: unit tests against MockGitRefClient for each operation; integration tests against real bare repo for acquire/release/heartbeat round-trip

2. **Add CLI subcommands in `src/cli/lease.rs`**
   - ACs: Group 4 (all 5 criteria)
   - Follow the nested subcommand pattern from `src/cli/reservations.rs`
   - Add `Claim` variant to `Commands` enum in `src/cli.rs` (or a `Lease` parent with subcommands)
   - Subcommands:
     - `lazyspec claim <doc-id> [--agent-id <id>] --json`: resolves doc type/id from the document path or shorthand, calls `LeaseEngine::acquire`, outputs lease JSON
     - `lazyspec release <doc-id> [--agent-id <id>] [--expected-holder <id>] --json`: calls `release` or `admin_release` depending on flags, outputs confirmation JSON
     - `lazyspec leases --json`: calls `query`, outputs all leases as JSON array
     - `lazyspec heartbeat <doc-id> [--agent-id <id>] --json`: calls `heartbeat`, outputs updated lease JSON
   - Agent ID defaults to `resolve_agent_id()` when `--agent-id` is not provided
   - On failure: structured JSON error with message, non-zero exit code
   - Register in `src/main.rs` dispatch
   - Verification: integration tests creating a temp project with coordination config, running claim/release/leases/heartbeat commands, asserting JSON output

3. **Add lease-gate enforcement to write commands**
   - ACs: Group 5 (all 3 criteria)
   - In `src/engine/store_dispatch.rs` or at the CLI layer (in `src/main.rs` or a shared pre-dispatch function):
     - Before `create`, `update`, `delete` dispatch: if `config.coordination` is `Some`, check that the agent holds a lease for the target document
     - If no lease held: return error "document is not claimed. Run `lazyspec claim <id>` first."
     - If lease held by current agent: proceed
     - If coordination is `None`: skip check entirely (backward compatible)
   - The check calls `LeaseEngine::query` or `resolve_ref` + `read_ref_blob` on the specific lease ref to verify holder matches current agent ID
   - Verification: integration test with coordination enabled — write without lease fails; write with lease succeeds. Integration test with coordination disabled — write proceeds without lease check.

## Test Plan

### Lease engine (unit tests in `src/engine/lease.rs`)
- Test acquire on unclaimed document succeeds, returns lease with correct agent/timestamps
- Test acquire on already-claimed document fails with appropriate error
- Test release by holder succeeds, deletes ref
- Test release by non-holder fails
- Test admin_release with matching expected_holder succeeds
- Test admin_release with non-matching expected_holder fails
- Test heartbeat by holder extends expiry, creates new commit parented on old
- Test heartbeat by non-holder fails
- Test force_acquire on expired lease (beyond grace period) succeeds
- Test force_acquire on expired lease (within grace period) fails
- Test force_acquire on non-expired lease fails
- Test query returns all leases across types

### Lease engine (integration tests in `tests/`)
- Test full acquire → heartbeat → release cycle against real bare repo
- Test acquire conflict: two "agents" competing for same lease
- Test force_acquire after expiry against real repo

### CLI subcommands (integration tests in `tests/`)
- Test `claim` outputs lease JSON with correct fields
- Test `release` after claim outputs confirmation
- Test `leases` lists active leases as JSON array
- Test `heartbeat` after claim outputs updated expiry
- Test claim on already-claimed document returns JSON error and non-zero exit

### Lease-gate (integration tests in `tests/`)
- Test `create` with coordination enabled and no lease: error
- Test `create` with coordination enabled and held lease: success
- Test `create` with coordination disabled (no `[coordination]` section): success without lease
- Test `update` and `delete` follow same pattern (parameterized or repeated)

## Notes

- The lease engine is generic over `R: GitRefOps`, making it testable with MockGitRefClient from Iteration A.
- CLI subcommands use top-level command names (`claim`, `release`, `leases`, `heartbeat`) rather than nested under a `lease` parent, matching the RFC's design (`lazyspec claim`, not `lazyspec lease claim`).
- The write gate could live at the CLI layer (simpler, checks before dispatching to engine) or in `store_dispatch.rs` (catches all write paths including future TUI writes). Prefer the engine layer for completeness, but the CLI layer is acceptable if the engine approach introduces unwanted coupling.
- Duration parsing for `lease_duration` and `grace_period` from the config strings (e.g., "60m", "2m") needs a simple parser — consider the `humantime` crate or a manual parse of `{number}{unit}` patterns.
