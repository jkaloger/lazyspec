---
title: Distributed lease protocol safety review
type: audit
status: draft
author: jkaloger
date: 2026-05-11
tags:
- leasing
- distributed
- safety
related:
- related-to: RFC-035
---

## Scope

Distributed safety review of the lease engine introduced by RFC-035. Covers `acquire`, `release`, `heartbeat`, `force_acquire`, and `query` against the protocol's stated safety properties (linearization at remote, fetch-before-check, clock-skew tolerance, partition behaviour, initial-ref CAS).

Subjects:

- `src/engine/lease.rs` — protocol logic
- `src/engine/git_ref.rs` — `GitRefOps` implementation (CAS primitives, push semantics)
- RFC-035 §"Distributed Safety Properties" — claims being audited

Audit type: protocol-simulation safety review. Six simulation scenarios run as parallel subagents over an extracted operation model (concurrent acquire, stale local state, clock skew, crash between steps, heartbeat vs force-acquire, network partition).

## Protocol model (as implemented)

```
acquire(type, id, agent, now):
  1. fetch_refs(REMOTE, "refs/lazyspec/leases/{type}/*", --prune)  # REMOTE, glob fetch+prune
  2. resolve_ref(refname) -> existing                              # LOCAL
  3. if existing != None: bail "lease held"                        # LOCAL gate
  4. create_ref_commit(refname, [lease.json])                      # LOCAL: update-ref refname commit_sha 0000...0000
  5. push_ref(REMOTE, refname)                                     # REMOTE: plain `git push` (NO --force, NO --force-with-lease)

release(type, id, agent):
  1. fetch_refs(REMOTE, refname)                                   # REMOTE, single-ref fetch (no prune scope)
  2. resolve_ref(refname) -> sha                                   # LOCAL
  3. read_ref_blob, verify holder                                  # LOCAL gate
  4. delete_remote_ref(REMOTE, refname) via `git push :refname`    # REMOTE blind delete (no CAS)
  5. delete_ref(refname)                                           # LOCAL blind delete

heartbeat(type, id, agent, now):
  1. resolve_ref(refname) -> old_sha                               # LOCAL ONLY, NO FETCH
  2. read_ref_blob, verify holder                                  # LOCAL gate
  3. create_commit(parent=old_sha) -> new_sha                      # LOCAL
  4. update_ref(refname, new_sha, old_sha)                         # LOCAL CAS (already mutates local)
  5. push_ref(REMOTE, refname)                                     # REMOTE: plain push, no CAS

force_acquire(type, id, agent, now):
  1. fetch_refs(REMOTE, refname)                                   # REMOTE
  2. resolve_ref, read_ref_blob                                    # LOCAL
  3. read_commit_timestamp(sha) -> last_touched                    # client-side committer date, baked into commit SHA
  4. if now <= last_touched + duration + grace: bail               # LOCAL gate, caller's local clock
  5. create_commit(parent=sha)                                     # LOCAL
  6. push_ref_with_lease(REMOTE, expected_old=sha)                 # REMOTE CAS via --force-with-lease
  7. update_ref(refname, new_sha, sha)                             # LOCAL CAS (after remote)
```

Asymmetries that drive the findings below:

- `acquire`: relies on plain-push rejection of non-fast-forward against an existing remote ref for mutual exclusion. No explicit remote CAS at step 5.
- `release`: deletes remote without CAS. Verification is against the LOCAL blob.
- `heartbeat`: no fetch. Local-CAS mutates local state BEFORE the remote push. Remote push has no CAS. Pushing to a deleted remote ref CREATES it.
- `force_acquire`: only operation that performs remote CAS (via `--force-with-lease`) and orders remote-before-local.
- `read_commit_timestamp` is the committer date of the lease commit, written by the client at `commit-tree` time. `GIT_COMMITTER_DATE` is not set anywhere in the codebase.

## Criteria

RFC-035 §"Distributed Safety Properties":

1. Linearization points: all writes use CAS at the git ref level.
2. Fetch-before-check: lease gate fetches from remote before checking lease state.
3. Clock-skew tolerance: `grace_period` absorbs NTP drift; commit timestamps used as expiry reference (RFC says "server-side, written at push time").
4. Network partition: local commits succeed, push fails, agent continues locally with a warning, coordination resumes on heal.
5. Initial ref creation: CAS with all-zeros SHA prevents two agents both seeing "ref does not exist" and both creating.

Each finding tags which property it violates.

## Findings

### Finding 1: Phantom lease resurrection via heartbeat

**Severity:** critical
**Location:** `src/engine/lease.rs:139-171` (`heartbeat`), `src/engine/git_ref.rs:213-220` (`push_ref`)
**Property violated:** 1 (linearization at remote), 2 (fetch-before-check).

**Description.** Heartbeat does not fetch. Its remote push is a plain `git push origin refname` with no CAS. When the remote ref no longer exists (because another agent legitimately force-acquired and then released, or because the holder's own release partially failed), the heartbeat push CREATES the remote ref pointing at the heartbeating agent's stale lease commit. The agent silently resurrects a lease nobody granted.

Reachable interleaving:

1. Agent A acquires STORY-001. Lease ref exists on remote.
2. A's lease expires. Agent B `force_acquire`s; remote ref now points at B's commit.
3. B `release`s; remote ref deleted.
4. A is still alive (orchestrator's heartbeat timer was not cancelled, or A never observed step 2). A's local ref still points at A's original lease commit.
5. A's heartbeat fires: local resolve returns A's sha, local blob still says holder=A, local CAS succeeds, push to non-existent remote ref CREATES it.
6. Daemon C polls, sees the (resurrected) lease, skips dispatching STORY-001.

Window: the entire heartbeat interval between B's release and A's next fetch. Heartbeat performs no fetches, so the window is unbounded in normal operation.

**Recommendation.** Change heartbeat step 5 from `git push` to `git push --force-with-lease=refname:<old_sha>`. `--force-with-lease` with a non-zero expected sha requires the remote ref to currently equal `old_sha`; an absent remote ref fails the lease and the push is rejected. Equivalent framing: heartbeat is an UPDATE, never a CREATE. This single change also closes finding 5 (heartbeat-vs-force-acquire local divergence on the B-then-A push ordering).

---

### Finding 2: Lease squat via `GIT_COMMITTER_DATE`

**Severity:** critical
**Location:** `src/engine/git_ref.rs:112-165` (`create_commit`), `src/engine/lease.rs:190-196` (force-acquire gate)
**Property violated:** 3 (clock-skew tolerance; the RFC claim that timestamps are "server-side, written at push time" is false — committer date is client-side, baked into the commit SHA).

**Description.** The force-acquire gate is `now <= last_touched + duration + grace` where `last_touched = read_commit_timestamp(sha)` is the committer date in the commit object. `GIT_COMMITTER_DATE` is honoured by `git commit-tree` and is not pinned by the codebase. An agent (malicious or misconfigured) can write a lease commit with `GIT_COMMITTER_DATE=+99h`, after which every other agent's force-acquire gate evaluates `now <= +99h + 62m` and bails. The lease becomes effectively permanent until the squatting agent releases it.

`lease.expires` is also written by the holder, also client-clock-derived, but it is currently IGNORED by force-acquire — the gate consults only the commit timestamp.

**Recommendation.** Three layered defences, in priority order:

1. Bound trust in committer dates: reject leases where `commit_ts > local_now + max_allowed_skew` (e.g. 5m). Cheap, defeats the squat scenario.
2. Cross-gate against `lease.expires` from the blob in addition to commit timestamp; take the max-conservative reading. An attacker now must forge two values.
3. Long-term: derive expiry from a server-issued signal (e.g. ref-update reflog timestamp, or a separate `refs/lazyspec/expiry/{type}/{id}` ref whose ref-update transaction time is queryable). Note that the RFC's stated mechanism ("commit timestamps, server-side, written at push time") is not implementable with stock git — commit objects are immutable and their timestamps are part of the SHA. The RFC's claim should be corrected.

---

### Finding 3: Asymmetric-partition split-brain with blind release delete

**Severity:** critical
**Location:** `src/engine/lease.rs:114-137` (`delete_lease`), `src/engine/lease.rs:139-171` (`heartbeat`)
**Property violated:** 4 (partition behaviour) compounded by 1 (no remote CAS on release).

**Description.** An asymmetric partition where A can fetch but not push admits silent lease theft:

1. A holds the lease. Asymmetric partition begins: A's pushes fail, fetches still work (or A simply doesn't fetch — heartbeat never does).
2. A heartbeats: local CAS succeeds and mutates local state; push fails with Err. Local now diverges from remote. The function returns Err but the local mutation persists.
3. From the remote's perspective, A's lease commit timestamp is stale. B force-acquires legitimately (remote = B's commit).
4. B releases; remote = absent.
5. Partition heals. A calls `release`. Step 1 fetches a single ref (`fetch_ref_optional(... refname)`), no prune scope. The local ref still points at A's drifted heartbeat chain; if the remote ref is absent, `fetch` of a missing ref does not delete the local ref. A's `resolve_ref` returns the stale local sha.
6. A reads the local blob (still says holder=A), passes the holder check, and executes `delete_remote_ref` — a blind `git push origin :refname`. Whatever the remote currently holds (a new lease from agent C, who acquired after B released) is deleted.

The blind remote delete is the proximate cause. The heartbeat's local-before-push ordering is the upstream cause that creates the divergence.

**Recommendation.**

1. `release` must use CAS on the remote delete: `git push --force-with-lease=refname:<verified_sha> origin :refname`. The verified sha should come from a `fetch + resolve` round-trip immediately before, not from local stale state.
2. `release` must fetch with the same glob-prune refspec as `acquire` (`refs/lazyspec/leases/{type}/*`, not a single ref), so absent remote refs prune local stale state.
3. Reorder `heartbeat`: push BEFORE local update_ref (mirror `force_acquire`'s order). Combined with finding 1's `--force-with-lease`, local state never advances past what the remote accepts.

---

### Finding 4: Clock-skew split-brain beyond `duration + grace`

**Severity:** high
**Location:** `src/engine/lease.rs:173-211` (`force_acquire`), commit-time derivation
**Property violated:** 3 (clock-skew tolerance).

**Description.** Define `Δ = clock_skew(force_acquirer) − clock_skew(holder)`. The force-acquire gate triggers when `now_force_acquirer > commit_ts_holder + duration + grace`. In true time, the holder believes the lease is valid until `acquired_true + duration`. Force-acquire fires prematurely when `Δ > duration + grace` — in the default configuration (60m + 2m), 62 minutes of skew between two machines admits split-brain for the difference window.

This is a property of the protocol, not an implementation bug. The RFC asserts `grace_period` "absorbs NTP drift", which is true for tight NTP (sub-second to minutes) but is not a safety guarantee in the presence of pathological skew (laptop suspended for an hour, VM clock drift, deliberate skew). Finding 2 makes adversarial skew trivial.

**Recommendation.** Document the skew budget explicitly: split-brain is reachable iff `|Δ_clocks| > duration + grace_period`. Either:

- Tighten the operational requirement (NTP mandatory; clamp `duration` to a multiple of expected drift).
- Pair with finding 2's mitigation: reject leases whose `commit_ts` is more than `max_skew` in the future. This bounds Δ from one side.
- Surface skew in `lazyspec leases` output (compare commit timestamp to local clock; warn if delta exceeds a threshold).

---

### Finding 5: `force_acquire` crash window self-locks the acquirer

**Severity:** high
**Location:** `src/engine/lease.rs:198-210`
**Property violated:** crash-safety (implicit in property 1).

**Description.** `force_acquire` pushes (step 6) BEFORE updating the local ref (step 7). If the process crashes between push success and local `update_ref`, the remote points at the acquirer's lease but the local ref still points at the previous holder's commit. On recovery:

- The same agent's next `heartbeat` resolves the local (stale) sha, reads the previous holder's `lease.json`, finds holder mismatch (`holder != self`), and bails with "lease held by X".
- The agent that ACTUALLY holds the remote lease cannot heartbeat against its own remote state without manual intervention (clear local ref, refetch).

The lease is "owned by no one operationally" until expiry-plus-grace elapses on commit time, at which point another `force_acquire` succeeds.

This is the right ordering for the remote (remote-first ensures local divergence on push failure is benign) but exposes a crash window.

**Recommendation.** On the next operation start by the same agent, run a recovery probe: fetch the lease ref, compare local vs remote. If they differ and remote holder matches `self`, reconcile local. Cheap implementation: heartbeat should fetch as a first step (also fixes finding 1's defence in depth).

---

### Finding 6: Heartbeat-vs-force-acquire local divergence (orphan refs)

**Severity:** medium
**Location:** `src/engine/lease.rs:139-171`
**Property violated:** 1 (no remote CAS) and operational hygiene.

**Description.** When A heartbeats while B has force-acquired the lease:

- B's push-with-lease succeeds first: A's subsequent push is rejected non-fast-forward; A's heartbeat returns Err but A's LOCAL ref has already advanced (step 4 ran before step 5). Each subsequent heartbeat extends A's local chain further from remote.
- On A's next release attempt, the fetch updates local to B's commit. Holder check fails. A bails forever. A's orchestrator never observes that A lost the lease unless it inspects the Err.

This is not a safety violation (B legitimately holds remote, A holds nothing). It is an operational liveness/observability gap: A wastes work, accumulates orphan commits in the local repo, and produces confusing logs.

**Recommendation.** On heartbeat push failure (non-fast-forward), fetch, reset local ref to remote, re-evaluate holder. If `holder != self`, drop the lease cleanly and surface the event to the caller. Finding 1's `--force-with-lease` push closes the safety side of this finding; the liveness side requires the additional fetch-on-failure handler.

---

### Finding 7: `acquire` crash between local update-ref and remote push

**Severity:** medium
**Location:** `src/engine/lease.rs:62-91`
**Property violated:** crash-safety.

**Description.** `acquire` updates the local ref (step 4) before pushing (step 5). A crash between the two leaves the local ref pointing at an unpushed lease commit. The next operation by the same agent:

- `heartbeat`: resolves the local sha, reads the local blob (holder matches), creates a heartbeat commit, pushes — to a non-existent remote ref → CREATES it. The agent retroactively wins an acquire it never completed, without re-running the acquire gate (no fetch, no expiry check).
- `release` or further `acquire`: the next `acquire`'s glob-fetch-with-prune cleans the stale local ref (because remote has nothing matching). Self-healing IF the next op is acquire.

The hazard is the heartbeat path. If another agent acquired in the crash window, that agent's push happened first; A's heartbeat push is then non-fast-forward and rejected — safe. The unsafe case is the empty-remote window: no other actor acquired, A's heartbeat resurrects the lease.

**Recommendation.** Push first, then local update-ref (mirror `force_acquire`). On push failure, do not advance local. Alternatively, on agent startup, audit local lease refs that have no remote counterpart and prune them.

---

### Finding 8: `release` does not glob-fetch with prune

**Severity:** medium
**Location:** `src/engine/lease.rs:122-127`, contrast with `src/engine/lease.rs:73-74`
**Property violated:** 2 (fetch-before-check is weaker on release than on acquire).

**Description.** `acquire` uses a glob refspec `refs/lazyspec/leases/{type}/*` so that `git fetch --prune` removes stale local refs whose remote counterparts are gone. `release` and `heartbeat`/`force_acquire`'s delete path use a single-ref fetch (`fetch_ref_optional(... refname)`). A single-ref fetch with `--prune` only prunes within the explicit refspec, not across the namespace; if the remote ref is absent, the local ref is NOT pruned. This is the precondition for finding 3.

**Recommendation.** `delete_lease` and `force_acquire` should fetch the same glob refspec used by `acquire` so absent-on-remote refs are pruned locally before resolution.

---

### Finding 9: `release` blind delete (no CAS)

**Severity:** medium (narrow), high (when triggered)
**Location:** `src/engine/lease.rs:133-136`, `src/engine/git_ref.rs:222-230`
**Property violated:** 1 (linearization at remote).

**Description.** `delete_remote_ref` is `git push origin :refname` — deletes the remote ref regardless of its current value. Verification is against the LOCAL blob (`lease.agent == expected_agent`). If local is stale (finding 3 + finding 8), a release operation can delete another agent's legitimate lease.

The current `fetch_ref_optional` only swallows "couldn't find remote ref" — network errors bubble. So the practical likelihood requires either an asymmetric partition (finding 3), an alternates corruption that returns success-but-stale, or a prune-scope gap (finding 8). With finding 8 unaddressed, the trigger is reachable in normal operation.

**Recommendation.** `delete_remote_ref` should grow an `expected_old` parameter and use `git push --force-with-lease=refname:<sha> origin :refname`. The CLI primitive exists; `push_ref_with_lease` is already implemented for `force_acquire`. The release path should plumb the verified sha through.

---

### Finding 10: Partition narrative mismatches implementation

**Severity:** low (documentation), but compounds findings 1/3/5
**Location:** RFC-035 §"Network partition behaviour" vs `src/engine/lease.rs`
**Property violated:** 4 (RFC claim vs implementation).

**Description.** RFC-035 states: "During a partition, local ref commits succeed but push fails. The lease gate falls back to local refs with a warning, allowing the agent to continue working locally." This narrative is implemented ONLY in `query` (`lease.rs:214-219`, which prints `eprintln!` and proceeds with local list). It is NOT implemented in `acquire`, `release`, `heartbeat`, or `force_acquire`:

- `acquire`/`release`/`force_acquire`: network errors from `fetch_ref_optional` bubble; the operation fails hard.
- `heartbeat`: never fetches, so it has no partition signal. Local-CAS mutates local state before the push fails. The function returns Err but local has already advanced — the worst-of-both-worlds outcome that drives finding 6 and feeds finding 3.

**Recommendation.** Either (a) implement the RFC narrative consistently — add an explicit "local-only mode" that surfaces a structured warning and a `pushed: false` flag, so the daemon can choose policy; or (b) revise RFC-035 to state that lease operations require remote connectivity and document the operational consequences. The `query` path's current "warning + local fallback" should be considered the exception, not the design contract.

---

### Finding 11: `acquire` initial-create relies on side-effect of plain push

**Severity:** low (UX/diagnostics; safety holds)
**Location:** `src/engine/lease.rs:87-89`, `src/engine/git_ref.rs:213-220`
**Property violated:** 5 (initial ref CAS).

**Description.** RFC-035 specifies "CAS with all-zeros SHA" for initial ref creation. The implementation uses plain `git push origin refname`, which succeeds when the remote ref is absent and fails as non-fast-forward when it exists. Safety holds — the remote serializes ref updates and the second concurrent push is rejected. But:

- The implementation conflates "remote rejected because the ref already exists" with "network/push failed for some other reason". Error class is muddied.
- The RFC's "all-zeros SHA" wording suggests explicit CAS at the push, which is `--force-with-lease=refname:0000...0000`. The current code uses CAS only at the LOCAL update-ref (`create_ref_commit` calls `update-ref refname commit_sha 0000...0000` locally) — local mutual exclusion within one clone, not across clones.

**Recommendation.** Either (a) route the initial acquire push through `push_ref_with_lease(expected_old=None)` (which already exists and emits `--force-with-lease=refname` with no expected value — but semantically this is closer to "I expect nothing specific"; needs the zero-sha form) or (b) extend `push_ref_with_lease` to accept a `Some("0000...0000")` and document this as the canonical initial-acquire form. Distinguishes "lease held" from "push failed" errors cleanly.

---

## Summary

The lease engine has the right shape for one operation (`force_acquire`: remote CAS, remote-before-local, fetch-before-gate) and an inconsistent shape elsewhere. The recurring root cause is that `heartbeat` and `release` perform remote writes without CAS, and `heartbeat` skips the fetch entirely. This creates three reachable split-brain or resurrection paths (findings 1, 3, plus the time-skew variant in 4) and several smaller hygiene issues.

Priority order if all findings are taken:

1. Finding 1 (heartbeat must `--force-with-lease`; closes findings 1 and most of 6).
2. Finding 3 (release must fetch-with-prune and delete-with-CAS; closes findings 8 and 9; closes finding 3 split-brain).
3. Finding 2 (committer-date sanity check; closes the squat vector and bounds finding 4).
4. Finding 5/7 (crash-window cleanup; small recovery probe at op start, or fetch-on-heartbeat).
5. Finding 10 (RFC alignment; either implement the narrative or revise it).
6. Finding 11 (explicit zero-sha CAS for initial acquire; cosmetic but improves error classes).

Findings 1, 2, and 3 are independently CRITICAL. They are reachable in normal operation (not edge cases requiring exotic failure modes) and the failure mode is silent: agents and the daemon observe a "valid" lease state that violates the protocol's safety invariant. A single root-cause fix (`--force-with-lease` on every remote write that targets an existing ref, with explicit zero-sha for creates) closes most of the surface.

Findings are presented for triage. Use `/create-iteration` against any subset the user selects.
