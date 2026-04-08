---
title: distributed-lease-protocol-hardening
type: iteration
status: accepted
author: jkaloger
date: 2026-04-07
tags: []
related:
- related-to: AUDIT-015
- related-to: STORY-108
- related-to: RFC-035
---





## Context

AUDIT-015 simulation of distributed scenarios found that the lease protocol has five failure modes when multiple agents operate from different machines. The document storage protocol (CAS-on-push) is sound. The lease protocol is not.

This iteration depends on ITERATION-155 (mechanical fixes) shipping first. ITERATION-155 fixes heartbeat CAS, orphan commits, agent ID, and missing CLI surface. This iteration fixes the distributed protocol bugs that remain after those mechanical fixes.

Requires an RFC-035 amendment: agent identity fallback changes from PID-based to stable identifier (covered in ITERATION-155 Task 2), and lease protocol operations gain explicit fetch requirements documented here.

## Failure modes addressed

| # | Scenario | Root cause | Severity |
|---|----------|-----------|----------|
| S7 | Lease gate bypass via stale local refs | `check_lease_gate` reads local refs, never fetches | High |
| S4 | Heartbeat vs force-acquire split-brain | `force_acquire` does delete + create (non-atomic) | High |
| S3 | Clock skew on force-acquire | Trusts acquirer's clock for `expires` | Medium |
| S8 | release/admin_release fail on stale clones | No fetch before `resolve_ref` | Medium |
| S1 | Acquire local CAS missing | `create_ref_commit` uses non-CAS `update-ref` for initial write | Low |

## Changes

### Task 1: Add `push_ref_with_lease` to `GitRefOps` trait

**Addresses:** S4 (atomic force-acquire)

**Files:**
- `src/engine/git_ref.rs` (trait, `GitCli` impl, `MockGitRefClient`)

**Implementation:**

Add a new trait method:
```rust
fn push_ref_with_lease(
    &self, root: &Path, remote: &str, refname: &str, expected_old: Option<&str>
) -> Result<()>;
```

`GitCli` implementation runs:
- When `expected_old` is `Some(sha)`: `git push --force-with-lease=<refname>:<sha> <remote> <refname>`
- When `expected_old` is `None`: `git push --force-with-lease=<refname> <remote> <refname>` (expects ref to not exist)

This gives atomic compare-and-swap at the remote. The push succeeds only if the remote ref matches `expected_old`. If another agent modified the ref between our read and our push, the push is rejected.

Update `MockGitRefClient` with a `push_with_lease_results` queue and call recording.

**Verification:** Unit test: mock records correct `--force-with-lease` args. Integration test: push-with-lease to a real remote succeeds when ref matches, fails when ref was changed.

### Task 2: Add fetch to lease gate

**Addresses:** S7 (stale gate bypass)

**Files:**
- `src/cli/lease.rs` (`check_lease_gate_with`)

**Implementation:**

In `check_lease_gate_with` (lease.rs line 53), before any `resolve_ref` or `list_refs` call, fetch the relevant lease refs from the remote:

1. When `doc_id` is `Some(id)`: fetch the specific lease ref with `fetch_ref_optional(git, root, remote, &refname)` before `resolve_ref` at line 69.
2. When `doc_id` is `None`: fetch the lease glob with `git.fetch_refs(root, remote, "refs/lazyspec/leases/")` before `list_refs` at line 89. Use a non-fatal fetch (if remote is unreachable, fall back to local refs with a warning).

The `check_lease_gate_with` function needs access to the coordination config's `remote` field. It currently takes `config: &Config` -- extract `remote` from `config.coordination`.

The fetch adds one remote round-trip per gated write. This is acceptable: gated writes already do remote operations (push_ref for git-ref documents, gh API calls for github-issues documents). For filesystem documents with coordination enabled, this is new latency. Document this tradeoff.

**Verification:** Integration test: Agent A claims doc, Agent B (simulated by deleting local lease ref) tries to write. Without fetch, gate passes (stale). With fetch, gate correctly rejects.

### Task 3: Make force_acquire atomic

**Addresses:** S4 (heartbeat vs force-acquire split-brain)

**Files:**
- `src/engine/lease.rs` (`force_acquire` method)

**Implementation:**

Replace the current three-step sequence (delete_remote_ref + delete_ref + create_ref_commit + push_ref) with an atomic swap:

1. Fetch the lease ref (already done at line 186).
2. Resolve and read the existing lease. Verify it's expired beyond grace period (lines 187-198, unchanged).
3. Record the current remote SHA from `resolve_ref`.
4. Create a new lease commit locally via `create_commit` (not `create_ref_commit` -- no local ref update yet). This is the commit with the new agent's `lease.json`.
5. Push with `push_ref_with_lease(root, remote, refname, Some(old_sha))`. This atomically replaces the remote ref only if it still points to `old_sha`. If another agent heartbeated or someone else force-acquired in the meantime, the push fails.
6. Only after the remote push succeeds, update the local ref with `update_ref(root, refname, new_sha, old_sha)`.

Remove the `delete_remote_ref` + `delete_ref` calls entirely. The force-with-lease push replaces the remote ref in one operation.

**Verification:** Unit test with mock: verify `push_ref_with_lease` is called with the old SHA, no `delete_remote_ref` calls. Integration test: two agents race on force-acquire of an expired lease, only one succeeds.

### Task 4: Add fetch to release and admin_release

**Addresses:** S8 (release fails on stale clones)

**Files:**
- `src/engine/lease.rs` (`delete_lease` method)

**Implementation:**

In `delete_lease` (line 118), add a fetch before `resolve_ref`:

```rust
fetch_ref_optional(&self.git, root, &self.config.remote, &refname)?;
```

Insert at line 126, before the `resolve_ref` at line 127. This ensures the orchestrator or admin on a different machine sees the current lease state before attempting to verify the holder and delete.

Also add fetch to `query` (line 216): fetch `refs/lazyspec/leases/` before `list_refs`. The `leases --json` output should reflect the remote state, not potentially stale local refs.

**Verification:** Integration test: claim on machine A, release from machine B (simulated by deleting local refs before release). Without fetch: "no lease found". With fetch: release succeeds.

### Task 5: Use server timestamps for expiry verification

**Addresses:** S3 (clock skew)

**Files:**
- `src/engine/lease.rs` (`force_acquire`, `Lease` struct)
- `src/engine/git_ref.rs` (new trait method or utility)

**Implementation:**

The current `force_acquire` compares `now` (the forcer's local clock) against `lease.expires` (set by the acquirer's local clock). If either clock is wrong, the comparison is wrong.

Add a method to read the committer timestamp from a commit:
```rust
fn read_commit_timestamp(&self, root: &Path, sha: &str) -> Result<DateTime<Utc>>;
```

`GitCli` implementation: `git cat-file -p <sha>` and parse the `committer ... <timestamp> <tz>` line.

In `force_acquire`, after reading the lease blob, also read the commit timestamp of the lease ref. Use the commit timestamp as the reference for "when was this lease last touched" instead of `lease.acquired`. The expiry check becomes:

```rust
let last_touched = self.git.read_commit_timestamp(root, &sha)?;
let effective_expiry = last_touched + duration + grace;
if now <= effective_expiry {
    bail!("lease not expired");
}
```

This doesn't eliminate clock skew entirely (the forcer's `now` is still local), but it removes one of the two clocks from the equation. The git server's committer timestamp is set by the machine that created the commit, which is the lease holder's machine. A heartbeat updates the committer timestamp, so `last_touched` reflects the most recent holder activity.

For full clock-skew immunity, the system would need a centralized time authority, which git doesn't provide. This is the pragmatic middle ground: one clock (the holder's) is in the data, the other (the forcer's) is local. The grace period absorbs the remaining skew.

**Verification:** Unit test: mock returns a commit timestamp, force_acquire uses it instead of `lease.acquired`. Integration test: create a lease commit with a known timestamp, verify force_acquire checks against the commit time.

### Task 6: CAS on initial ref creation

**Addresses:** S1 (local acquire race)

**Files:**
- `src/engine/git_ref.rs` (`create_ref_commit`)

**Implementation:**

Change `create_ref_commit` (line 145) to use CAS when creating the initial ref. Replace:
```rust
self.run_git(root, &["update-ref", refname, &commit_sha])
```
with:
```rust
self.run_git(root, &["update-ref", refname, &commit_sha, &"0".repeat(40)])
```

The all-zeros SHA as the old value tells `git update-ref` to fail if the ref already exists. This prevents two concurrent processes on the same clone from both creating the same ref.

This is a one-line change but closes the TOCTOU gap between `resolve_ref` returning `None` and `update-ref` creating the ref.

**Verification:** Unit test: two concurrent `create_ref_commit` calls on the same refname, second one fails. (Hard to test deterministically; a sequenced test where the ref is pre-created suffices.)

### Task 7: Amend RFC-035

**Files:**
- `docs/rfcs/RFC-035-git-ref-document-storage-with-lease-based-claiming.md`

**Implementation:**

Update the following RFC sections to match the hardened protocol:

1. **Lease Operations table** (line 116-123): Add "Fetch from remote" as a prerequisite to all operations. Update Force-acquire mechanism to "atomic ref swap via `push --force-with-lease`" instead of "delete and reacquire".

2. **Agent Identity** (line 261-267): Update fallback from "git config user.name + sqids-encoded PID" to "git config user.name" (per ITERATION-155 Task 2).

3. Add a new subsection **Distributed Safety Properties**:
   - All writes use CAS (document storage) or force-with-lease (leases) as linearization points
   - The lease gate fetches before checking, adding one remote round-trip per gated write
   - Clock skew tolerance: grace_period absorbs NTP drift; commit timestamps used for expiry reference
   - Network partition behavior: local writes succeed, push fails, lease gate falls back to local refs with warning

4. **GitRefOps trait** (line 290-301): Add `push_ref_with_lease` and `read_commit_timestamp` to the trait listing.

**Verification:** `cargo run -- validate --json` passes.

## Test Plan

### Unit tests

| Test | Verifies | Properties |
|------|----------|------------|
| `push_with_lease_passes_expected_sha` | Task 1: mock records `--force-with-lease=ref:sha` | Isolated, behavioral |
| `push_with_lease_none_expects_nonexistent` | Task 1: `None` old value uses bare `--force-with-lease` | Isolated, specific |
| `gate_fetches_before_resolve` | Task 2: mock records `fetch_refs` call before `resolve_ref` | Isolated, behavioral |
| `gate_falls_back_to_local_on_fetch_failure` | Task 2: fetch fails, gate still checks local refs with warning | Isolated, behavioral |
| `force_acquire_uses_push_with_lease` | Task 3: no `delete_remote_ref` calls, `push_ref_with_lease` called with old SHA | Isolated, behavioral |
| `force_acquire_fails_if_ref_changed` | Task 3: push_with_lease returns error, force_acquire propagates | Isolated, specific |
| `delete_lease_fetches_before_resolve` | Task 4: mock records fetch call | Isolated, behavioral |
| `query_fetches_before_list` | Task 4: mock records fetch before list_refs | Isolated, behavioral |
| `force_acquire_uses_commit_timestamp` | Task 5: mock returns commit timestamp, used instead of lease.acquired | Isolated, specific |
| `create_ref_commit_fails_if_ref_exists` | Task 6: pre-existing ref causes CAS failure | Isolated, specific |

### Integration tests

| Test | Verifies |
|------|----------|
| `two_agents_race_acquire` | Task 6: simulate two acquires, second push fails |
| `force_acquire_atomic_swap` | Task 3: force-acquire while another agent heartbeats, no split-brain |
| `gate_rejects_after_remote_release` | Task 2: local refs stale, gate fetches and rejects |
| `release_from_different_clone` | Task 4: release works after fetch on a clone that never saw the claim |

### Test tradeoffs

The integration tests for Tasks 2-4 need to simulate "different machines" on a single machine. Two approaches:
- **Two worktrees:** `git worktree add` gives two independent working directories with shared object store but independent refs. This accurately simulates two clones fetching from the same remote.
- **Ref manipulation:** Delete local refs to simulate a stale clone. Simpler but doesn't test the full fetch path.

Recommend worktree-based tests for Tasks 2 and 4 (where fetch correctness matters). Ref manipulation for Task 3 (where the race is between push operations, not local state).

## Notes

- Task 1 (push_ref_with_lease) is the foundation. Tasks 3 depends on it directly. Ship Task 1 first.
- Tasks 2 and 4 (add fetch to gate/release/query) are independent and can be done in parallel.
- Task 5 (commit timestamps) is the weakest fix -- it reduces clock skew exposure but doesn't eliminate it. Full elimination would require a centralized time authority, which is out of scope for a git-based system. The 2-minute grace period remains the primary buffer.
- Task 7 (RFC amendment) should be done last, after all code changes are verified.
- After this iteration, the remaining distributed risk is network partition (S6). This is inherent to the CAP theorem and acknowledged in RFC-035's graceful degradation section. No code fix exists for it.
