---
title: git-ref store and lease engine post-fix re-verification
type: audit
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- supersedes: AUDIT-015
- related-to: RFC-035
- related-to: STORY-108
- related-to: STORY-109
---

## Scope

Bug bash audit of the git-ref storage backend and lease engine against the current code (`b44da80`). Re-verifies AUDIT-015's seven findings (A-G), plus full surface coverage of RFC-035 via distributed-protocol simulation. Supersedes AUDIT-015.

Surface covered:
- Lease lifecycle: `acquire`, `release`, `admin_release`, `heartbeat`, `force_acquire`, `query`
- Lease-gate enforcement on writes (all backends)
- `GitRefStore` CRUD via commit chains, shadow cache, `cache.lock`
- `lazyspec fetch` materialization
- Cross-backend reads (filesystem, github-issues, git-ref)
- Agent identity resolution
- `link` command on git-ref docs
- `--json` coverage across CLI commands

Method: protocol model extraction + parallel Opus simulation across the eight required scenarios (concurrent acquire, stale local state, clock skew, crash between steps, concurrent release+acquire, non-atomic replace, heartbeat vs eviction, network partition) plus targeted code review.

## Criteria

- RFC-035 design properties: linearization at the remote, fetch-before-check, clock-skew tolerance, atomic replace, recoverable crash semantics.
- Codebase Principle 2: every command supports `--json`.
- AUDIT-015 findings A-G must remain fixed.
- No new regressions introduced by AUDIT-014 fixes.

## AUDIT-015 Re-verification

| AUDIT-015 Finding | Status | Evidence |
|---|---|---|
| A: Agent ID PID non-determinism | FIXED | `src/engine/agent.rs:5-41` uses plain `git config user.name`, no PID/sqids component. |
| B: Heartbeat CAS conflict | FIXED | `src/engine/lease.rs:137-169` uses `create_commit` (no ref move) then `update_ref` CAS. New issue introduced: see Finding 2. |
| C: Update creates orphan commits | FIXED | `src/engine/git_ref_store.rs:194-199` passes `Some(&old_sha)` to `create_commit`. |
| D: `link` bypasses git ref | PARTIALLY FIXED | `src/cli/link.rs:69` calls `push_if_git_ref_backed` which updates the local ref. Remote push step missing -- see Finding 13. |
| E: `force_acquire` not exposed via CLI | FIXED | `src/cli/lease.rs:115-143` `run_claim` accepts `force: bool`. |
| F: `update` missing `--json` | FIXED | `src/main.rs:151-181` `Update` accepts `json` and emits doc JSON. |
| G: Number reuse after delete | NOT FIXED | `src/engine/git_ref_store.rs:52-67` still uses `max(existing_refs) + 1`. |

## Findings

### Finding 1: Concurrent acquire produces split-brain leases

**Severity:** critical
**Location:** `src/engine/lease.rs:62-89` (`acquire`), `src/engine/git_ref.rs:167-183` (`create_ref_commit`), `src/engine/git_ref.rs:213-220` (`push_ref`)
**Description:**
`acquire` treats the local `update-ref` (with all-zeros CAS) as the linearization point. The local CAS only proves no other process on the same machine raced; it says nothing about the remote. Two agents on different machines fetch (both see no ref), resolve (both `None`), `create_ref_commit` (both succeed locally with zero-SHA CAS), then push.

Failure modes:
1. **Both pushes succeed:** when the remote allows non-fast-forward to `refs/lazyspec/leases/*` (custom refs are unprotected by default on GitHub/GitLab), the second push silently overwrites the first. Both agents return `Ok(lease)`. Split-brain.
2. **Second push rejected:** loser's local ref is left at its own (orphan) lease commit; loser returns `Err`, but its local state shows it as the holder until next fetch. `query()` on loser's machine reports a phantom lease.

The intended linearization point is the remote, not local CAS.

**Recommendation:**
Use `push_ref_with_lease(expected_old=None)` (`--force-with-lease=ref:`) at the remote as the gate. Build the commit object via `create_commit` (no ref move). Push first; only update the local ref after push acks. On push rejection, do not leave a local ref behind. Mirror `force_acquire`'s shape, but reorder so local update follows remote ack.

### Finding 2: Heartbeat skips fetch and uses non-CAS push

**Severity:** high
**Location:** `src/engine/lease.rs:137-169`
**Description:**
`heartbeat` does not fetch before reading the local ref. Sequence after eviction:
1. T0+62m: A's lease expires beyond grace.
2. B successfully `force_acquire`s. Remote ref now at S2.
3. A's local ref still at S1 (no fetch).
4. A heartbeats: `resolve_ref` returns S1, agent check passes (S1's blob still names A). `create_commit(parent=S1)` -> S3. `update_ref` CAS(S3, S1) succeeds locally. `push_ref(remote)` runs plain `git push`.
5. Remote at S2; S3 is not a descendant. Plain push rejected as non-fast-forward. A's local ref stays at S3, drifted from remote.
6. Subsequent heartbeats build S4 on S3, drift accumulates.

Self-healing only on next force-fetch (`fetch_refs` uses `+pattern:pattern` refspec at `git_ref.rs:204`); until then A's local view shows itself as holder while remote shows B.

Additional risk: plain `push_ref` is vulnerable to clobber on remotes that allow non-FF push for custom refs (default behavior on most hosted git for refs outside `refs/heads/`).

**Recommendation:**
- `fetch_refs` before `resolve_ref` in heartbeat.
- Use `push_ref_with_lease(expected_old=Some(old_sha))` instead of plain `push_ref`.
- On push failure, rollback the local CAS (`update_ref` back to `old_sha`) before returning the error.

### Finding 3: force_acquire vulnerable to clock skew

**Severity:** high
**Location:** `src/engine/lease.rs:171-209`, `src/engine/git_ref.rs:143-156` (`create_commit` runs `commit-tree` with no `--date`), `src/cli/lease.rs:50-105` (lease_gate has no time check)
**Description:**
Three sub-issues:

a) `force_acquire` uses commit timestamp (`read_commit_timestamp`) as `last_touched`, not the server-set `lease.expires` field in the JSON blob (line 186 binds it to `_lease`, unused). When agent B's clock is forward of agent A's by more than `grace_period (2m)`, B's `now > effective_expiry` even though A's lease has real wall-clock time remaining. Spurious force-acquire.

b) `commit-tree` is invoked with no `--date` and no `GIT_COMMITTER_DATE` scrubbing. A buggy or malicious agent setting `GIT_COMMITTER_DATE=1970-01-01` produces a lease that any other agent can immediately steal, regardless of actual elapsed time.

c) `check_lease_gate_with` (`src/cli/lease.rs:67-78`) reads `lease.agent` and compares to caller. It never checks `lease.expires` nor commit timestamp. A holder whose own clock has drifted past their lease's wall-clock expiry continues writing through the gate.

`grace_period=2m` is a coincidence-tolerance margin, not a safety floor against NTP-skewed fleets.

**Recommendation:**
- `force_acquire` should compute `last_touched = max(commit_timestamp, lease.acquired)` from the JSON blob, requiring both signals to agree before stealing.
- Pass `--date` explicitly to `commit-tree` using the server-provided `now` value (already plumbed through the call), eliminating env-var influence.
- `check_lease_gate` must verify `now <= lease.expires + grace` to fail closed when the holder's clock has drifted past their own lease deadline.
- Document NTP requirement in `.lazyspec.toml` docs. Consider raising default `grace_period` to `5m`.

### Finding 4: force_acquire crash between push and local CAS leaves agent without lease handle

**Severity:** high
**Location:** `src/engine/lease.rs:171-209`
**Description:**
`force_acquire` order:
1. `create_commit(parent=old_sha)` LOCAL
2. `push_ref_with_lease(expected=old_sha)` REMOTE (succeeds)
3. `update_ref` CAS(new, old) LOCAL

If the process dies between steps 2 and 3, the remote ref is moved to the agent's new lease commit, but the local ref still points at the previous SHA. The agent has no in-memory record of holding the lease.

Recovery is broken:
- Next `acquire`: zero-SHA CAS fails (local ref exists).
- Next `force_acquire`: `fetch_refs` updates local to remote; ts check sees a fresh lease (the agent's own), refuses to steal.
- Next `release`: succeeds, but the agent must know to call it.
- Other agents: blocked until expiry, then can steal -- self-healing after a full lease TTL.

No reconcile command exists. The agent is silently the remote holder for up to 60 minutes with no way to act on it.

**Recommendation:**
- Reorder: do local `update_ref` BEFORE the remote push; on remote push failure, roll back local `update_ref`. Linearization moves to local, but with a sync rule: a successful local CAS without successful push must trigger a reconcile path.
- Or: introduce `lazyspec lease reconcile` that fetches all `refs/lazyspec/leases/*`, identifies refs owned by self with no in-memory handle, and surfaces them for adoption or release.

### Finding 5: Number reuse after delete (AUDIT-015 Finding G unfixed)

**Severity:** high
**Location:** `src/engine/git_ref_store.rs:52-67` (`next_number_from_refs`)
**Description:**
`next_number_from_refs` computes `max(existing_refs) + 1` over live refs only. After deleting NOTE-005 with NOTE-001-004 still present, the next allocation reuses NOTE-005. If the deleted document was pushed and other agents reference it via cached IDs, branches, or PR descriptions, the new doc collides with the old identity.

Filesystem-backed types avoid this via the reservation system (`refs/reservations/{PREFIX}/{NUM}`), which git-ref types do not consult.

**Recommendation:**
Maintain a high-water-mark ref under `refs/lazyspec/numbering/{type}` written on create, never decremented. Or integrate `GitRefStore::create` with the reservation system before falling back to `max+1`.

### Finding 6: doc.update and doc.create skip fetch, use plain push

**Severity:** medium
**Location:** `src/engine/git_ref_store.rs:91-150` (create), `:152-219` (update), `:221-288` (set_provenance)
**Description:**
`doc.update` reads `cache.lock` for `old_sha` and proceeds without fetching the remote. Concurrent updates from another agent on the same doc invalidate the local view. The local CAS on `update_ref` succeeds (local ref still matches `old_sha`), then `push_ref` is plain and rejected non-fast-forward when remote has moved. Local ref drifts ahead of remote at a divergent commit. `cache.lock` is updated to the divergent SHA at line 215. Future updates compound the divergence.

`doc.create` is similar: `next_number_from_refs` reads local refs only. Two agents both pick the same next number, both create local refs at zero-SHA CAS, both push. Loser's push rejected; loser's local ref orphaned with content lost unless retried.

The lease gate is supposed to prevent this for coordination-configured projects -- but only for git-ref types in lease-gated mode. Without `[coordination]`, no gate runs and divergence is unchecked.

**Recommendation:**
- All `GitRefStore` writes that produce non-FF commits should use `push_ref_with_lease(expected_old=Some(old_sha))`. On rejection, surface "your local view is stale, refetch and retry" instead of silently leaving the divergent state.
- `doc.update` should re-resolve the ref immediately before commit; CAS on the live ref SHA, not on `cache.lock`.
- `doc.create` should fetch before computing `next_number`, or use the reservation primitive (Finding 5).
- On any push rejection, automatically reset local ref to pre-write state so users can retry without manual git surgery.

### Finding 7: delete_remote_ref unconditional, races with concurrent force_acquire

**Severity:** medium
**Location:** `src/engine/lease.rs:112-135` (`delete_lease`), `src/engine/git_ref.rs:222-230` (`delete_remote_ref`)
**Description:**
`delete_lease` (used by both `release` and `admin_release`) reads the lease blob, checks the agent matches, then issues `git push origin :ref` -- a force-delete with no expected_old verification.

Race window: between reading the blob (line 126) and pushing the delete (line 132), the ref could be rotated. Scenarios:
- A's lease expires past grace. B successfully `force_acquire`s. A finishes its `release`, having read the blob before B's force-acquire. A's `delete_remote_ref` deletes B's lease.
- Orchestrator's `admin_release` reads lease showing dead agent-7. Concurrently agent-7 heartbeats (rotating to S2) or another agent force-acquires. Admin's delete clobbers.

Outcome: silent lease loss, split-brain (the new holder thinks it holds, ref is gone, third agent can re-acquire).

**Recommendation:**
Add `delete_remote_ref_with_expected(expected_old: &str)` that runs `git push --force-with-lease=ref:expected_sha origin :ref`. `delete_lease` already has the SHA at line 122. Use it. Failure becomes a loud "ref rotated, retry" instead of a silent overwrite.

### Finding 8: lease_gate's RFC-035 contradiction on partition

**Severity:** medium
**Location:** `src/cli/lease.rs:50-105` (`check_lease_gate_with`), `src/engine/lease.rs:38-50` (`fetch_ref_optional`), RFC-035 § Graceful Degradation
**Description:**
`check_lease_gate_with` calls `fetch_ref_optional`, which suppresses only "couldn't find remote ref" and propagates all other errors. On any network failure the gate fails closed.

RFC-035 § Graceful Degradation says: "Git-ref create/update: local ref commit succeeds. Push fails but document is readable locally." The code is fail-closed; the RFC implies fail-open with deferred push. They disagree.

The conservative behavior (fail-closed) is correct: relaxing the gate during partition reintroduces split-brain across partitioned agents both holding stale local "valid" leases. The RFC text is the bug.

**Recommendation:**
Amend RFC-035 to scope "git-ref create/update succeeds locally" to the no-coordination branch only. Lease-gated writes must remain fail-closed during partition.

### Finding 9: No reconcile path for crashed multi-step ops

**Severity:** medium
**Location:** absence -- no command exists
**Description:**
Multiple operations leave the system in states that require manual reconciliation:
- `force_acquire` crashed between push and local CAS (Finding 4): agent silently holds remote lease for up to 60m.
- `release`/`admin_release` crashed between remote delete and local delete: agent locked out of own document until local ref pruned.
- `doc.update` crashed between push and cache write: cache file disagrees with ref.
- `doc.create` crashed between push and cache write: ref exists, doc invisible to cache-driven listing.
- `doc.delete` crashed between remote delete and local delete: ghost doc visible from cache.

**Recommendation:**
Two reconcile commands:
- `lazyspec lease reconcile`: fetches all `refs/lazyspec/leases/*`, identifies remote leases owned by self with no live handle, surfaces them for adoption or release; prunes orphan local lease refs without remote counterparts.
- `lazyspec doc reconcile-cache`: rebuilds cache files and `cache.lock` from authoritative refs after a crashed update/create/delete.

### Finding 10: link.rs `push_if_git_ref_backed` does not push to remote

**Severity:** medium
**Location:** `src/cli/link.rs:181-232`
**Description:**
`push_if_git_ref_backed` reads the cache file, builds a new commit via `GitCli::create_commit`, calls `update_ref` with CAS, and writes the new SHA to `cache.lock`. It never calls `push_ref`. With coordination configured, a `link` on a git-ref doc updates the local ref but leaves the remote stale. Compare with `GitRefStore::update` (`git_ref_store.rs:208-209`) which pushes after update.

**Recommendation:**
Either route `link` for git-ref docs through `GitRefStore::update`, or add the matching `push_ref` call in `push_if_git_ref_backed` when coordination is configured.

### Finding 11: `--json` missing on Delete, Link, Unlink, Ignore, Unignore, Setup

**Severity:** medium
**Location:** `src/main.rs:81` (Setup), `:183` (Delete), `:190` (Link), `:210` (Unlink), `:230` (Ignore), `:236` (Unignore)
**Description:**
These commands destructure no `json` field. Codebase Principle 2 requires every command to support `--json`. Mutating commands (Delete, Link, Unlink) are the highest-impact gap because orchestrators cannot programmatically confirm success or read the resulting state.

**Recommendation:**
Add `--json` to each. Mutating commands return the updated doc (or list of affected docs) as JSON. Setup returns the resolved config and remote state. Ignore/Unignore return the updated doc.

### Finding 12: doc.create/update next_number race without coordination

**Severity:** medium
**Location:** `src/engine/git_ref_store.rs:52-67`, `:91-150`
**Description:**
Without `[coordination]`, `next_number_from_refs` is the only collision guard, and it's local-only. Two concurrent creates on different machines pick the same number, both push, second push rejected. The losing agent's content is in an orphan local commit no one else can see, and there is no automatic retry. This is a special case of Finding 6 but worth calling out: the reservation system exists precisely for this purpose for filesystem types and is not used by git-ref.

**Recommendation:**
Same as Finding 5 -- integrate git-ref creates with the reservation system or a high-water-mark ref.

### Finding 13: IssueCache.is_fresh silently returns false on SHA values

**Severity:** low
**Location:** `src/engine/issue_cache.rs:60`
**Description:**
`is_fresh` parses the lock value as `DateTime<Utc>`. Git-ref entries store SHAs, not timestamps. The parse fails and `is_fresh` returns false. Harmless for current usage (IssueCache only queries github-issues keys), but if a github-issues type and a git-ref type share an ID prefix (e.g., both `STORY-*`), IssueCache would treat the git-ref lock entry as a stale github cache entry.

**Recommendation:**
`is_fresh` should distinguish lock value formats (SHA vs RFC3339) and ignore non-timestamp entries instead of parsing them. Or partition the lock key namespace explicitly per backend.

### Finding 14: Cache file path collision risk between same-named types

**Severity:** low
**Location:** `src/engine/store.rs:55-89`, `src/engine/git_ref_store.rs`, `src/engine/issue_cache.rs`
**Description:**
Both `git-ref` and `github-issues` backends write to `.lazyspec/cache/{type_name}/{id}.md`. The config parser prevents duplicate type names today, but no runtime assertion protects against misconfiguration if that constraint is loosened.

**Recommendation:**
Add a runtime check at config load that no two types share a `name` regardless of backend. Consider partitioning cache paths by backend for defense in depth: `.lazyspec/cache/{backend}/{type_name}/{id}.md`.

## Summary

AUDIT-015 produced seven findings (A-G); six are fixed in current code, one (G, number reuse) persists. The fixes are correct.

The deeper protocol simulation surfaces fourteen new findings across the lease and git-ref store. The pattern: lease and write paths treat **local** state as authoritative when the **remote** is the only correct linearization point. Fetch-before-check is missing on heartbeat. Push is plain (not `--force-with-lease`) on most write paths. Crash recovery has no reconcile command. The grace period (2m) is treated as a clock-skew floor when it is only a coincidence margin.

Prioritised remediation:

1. **Finding 1 (critical):** rebuild `acquire` around remote-as-linearization-point. Without this, the entire claim primitive is unsafe.
2. **Findings 2, 3, 4 (high):** heartbeat fetch+CAS-push, force_acquire clock-skew defense (use `lease.expires` as primary signal, set commit `--date` explicitly), reconcile path for crashed force_acquire.
3. **Findings 5, 6, 7 (high/medium):** number reuse, doc-write divergence, unconditional delete -- all addressable by a consistent "use `--force-with-lease` everywhere; integrate with reservation primitive" pass.
4. **Findings 8-14 (medium/low):** RFC-035 amendment, reconcile commands, `link` push, `--json` parity, namespace defenses.

The codebase Principle 1 ("produce, validate, and serve structured markdown") and Principle 2 ("every command supports `--json`") are both at risk: split-brain on the lease primitive corrupts the markdown invariants the system is supposed to enforce, and six mutating commands lack JSON output. Both must close before the lease/git-ref surface can be considered 1.0-ready.
