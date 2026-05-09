---
title: git-ref-store-manual-test-plan
type: audit
status: complete
author: jkaloger
date: 2026-04-06
tags: []
related:
- related-to: STORY-108
- related-to: STORY-109
- related-to: RFC-035
- supersedes: AUDIT-014
---






## Scope

Manual test plan for RFC-035 git-ref storage and lease engine. Audit type: bug bash.

Covers STORY-108 (GitRefOps trait, lease engine, CLI lease commands, agent identity, lease-gate enforcement) and STORY-109 (GitRefStore, shadow cache, fetch, cross-backend reads, cold cache fallback). All tests run against a real git repo with `origin` as the remote.

Supersedes AUDIT-014, which found three issues (write dispatch bypass, cache.lock format conflict, lease first-use failure). All three were fixed in `b7b5d7a`. This plan re-verifies those fixes and covers the full surface area.

## Prerequisites

Add a git-ref type to `.lazyspec.toml` for testing. Use a throwaway type name `note` to avoid polluting real document types:

```toml
[[types]]
name = "note"
plural = "notes"
dir = "docs/notes"
prefix = "NOTE"
icon = "📝"
store = "git-ref"

[coordination]
remote = "origin"
lease_duration = "60m"
grace_period = "2m"
max_push_retries = 5
```

After testing, remove the `note` type and `[coordination]` section, and clean up any refs with `git push origin --delete refs/lazyspec/note/*` and `git push origin --delete refs/lazyspec/leases/*`.

## Criteria

Each test case is a self-contained step. Run sequentially within a group; groups are independent. Mark each PASS/FAIL during execution.

## Findings

### New issues discovered

#### Finding A: Agent ID non-determinism breaks lease continuity across CLI invocations

**Severity:** high
**Location:** `src/engine/agent.rs:41`
**Description:** The git-config fallback encodes the current PID via sqids, producing a different agent ID for every process. A lease acquired by one `cargo run` invocation cannot be verified by the next, because the agent IDs differ. This makes the lease gate unusable without setting `$LAZYSPEC_AGENT_ID` or `$CLAUDE_SESSION_ID`. The env vars work, but the fallback path is broken by design for multi-invocation workflows.
**Recommendation:** Use a stable per-session identifier for the fallback. Options: hash of TTY + shell PID, a file-based session token in `.lazyspec/`, or drop the PID component entirely and use just `git config user.name` (accepting that two terminals by the same user share an identity).

#### Finding B: Heartbeat always fails (CAS conflict)

**Severity:** high
**Location:** `src/engine/lease.rs:168-172`, `src/engine/git_ref.rs:145-159`
**Description:** `heartbeat` calls `create_ref_commit` (which internally does a non-CAS `update-ref`, moving the ref to the new commit), then calls `update_ref` with CAS using the old SHA. By the time CAS runs, the ref has already been moved by `create_ref_commit`, so the old SHA is stale and CAS fails with "is at X but expected Y". Heartbeat is completely broken.
**Recommendation:** Either (a) have `heartbeat` call `create_commit` (without the ref update) followed by `update_ref` with CAS, or (b) add a `create_commit_only` method that builds the commit object without touching the ref, then let `heartbeat` do the CAS update itself.

#### Finding C: `update` creates orphan commits instead of commit chains

**Severity:** medium
**Location:** `src/engine/git_ref_store.rs` (update path), `src/engine/git_ref.rs:145`
**Description:** `create_ref_commit` creates an orphan commit (no parent). RFC-035 specifies that updates should create commits parented on the previous, giving per-document history. Currently each update replaces the entire ref with an unrelated orphan commit, losing the document's change history.
**Recommendation:** Pass the current ref SHA as a parent when creating the update commit. `git commit-tree` accepts `-p <parent>`.

#### Finding D: `link` writes to cache file, not to git ref

**Severity:** medium
**Location:** `src/cli/` (link command)
**Description:** `lazyspec link` on a git-ref document modifies the cache file on disk but does not update the underlying git ref. The relationship is lost on the next `fetch` or cold cache fallback, which re-materializes from the ref (which still has the old content).
**Recommendation:** Route `link` through `GitRefStore::update` for git-ref documents, the same way `update` and `delete` were fixed in AUDIT-014.

#### Finding E: `force_acquire` not exposed via CLI

**Severity:** medium
**Location:** `src/engine/lease.rs:177`, `src/cli/lease.rs`
**Description:** `LeaseEngine::force_acquire` is implemented and unit-tested but not wired to any CLI flag. `claim` always calls `acquire`, which rejects held leases regardless of expiry. There is no way to reclaim an expired lease from the command line.
**Recommendation:** Add `--force` flag to `claim` that calls `force_acquire` when the lease is expired.

#### Finding F: `update` command missing `--json` flag

**Severity:** low
**Location:** `src/main.rs` (update command definition)
**Description:** `lazyspec update` does not accept `--json`. Every other mutating command does. This violates Principle 2 ("every command supports `--json`").
**Recommendation:** Add `--json` flag to `update`.

#### Finding G: Number reuse after delete

**Severity:** low
**Location:** `src/engine/git_ref_store.rs:51` (`next_number_from_refs`)
**Description:** `next_number_from_refs` computes `max(existing_refs) + 1`. After deleting NOTE-002 with NOTE-001 still present, the next allocation is NOTE-002 (reusing the deleted number). If the deleted document was pushed to a remote and other agents still reference it, this creates an ID collision. Filesystem-backed types avoid this via the reservation system, which git-ref types don't use.
**Recommendation:** Either integrate with the reservation system or track a high-water mark in a separate ref.

### Test results

| # | Test | Result |
|---|------|--------|
| 1 | Create git-ref document | PASS |
| 2 | Show git-ref document | PASS |
| 3 | Update git-ref document | PARTIAL (orphan commit, see Finding C) |
| 4 | Delete git-ref document | PASS |
| 5 | Number allocation after delete | INFO (reuses deleted number, see Finding G) |
| 6 | Cold cache fallback | PASS |
| 7 | Fetch materializes cache | PASS |
| 8 | Fetch removes deleted refs | PASS |
| 9 | Fetch with no remote refs | PASS |
| 10 | List cross-backend | PASS |
| 11 | Search cross-backend | PASS |
| 12 | Validate cross-backend | PASS |
| 13 | Status cross-backend | PASS |
| 14 | Context cross-backend | PASS (but link doesn't persist to ref, see Finding D) |
| 15 | First-time claim | PASS (AUDIT-014 Finding 3 fix confirmed) |
| 16 | Claim conflict | PASS |
| 17 | Heartbeat | FAIL (CAS always fails, see Finding B) |
| 18 | List leases | PASS |
| 19 | Release (owner) | PASS |
| 20 | Release (non-holder rejected) | PASS |
| 21 | Admin release | PASS |
| 22 | Force-acquire expired | FAIL (not exposed via CLI, see Finding E) |
| 23 | LAZYSPEC_AGENT_ID priority | PASS |
| 24 | CLAUDE_SESSION_ID fallback | PASS |
| 25 | git config fallback | PASS (but non-deterministic, see Finding A) |
| 26 | Write refused without lease | PASS |
| 27 | Write with lease held | PASS |
| 28 | Write without coordination | PASS |
| 29 | cache.lock mixed format | PASS (migrated on write) |
| 30 | Cache gitignored | PASS |

## Summary

23/30 PASS, 2 FAIL, 2 PARTIAL/INFO, 3 not fully testable.

The read path is solid: cold cache fallback, fetch, cross-backend list/show/search/validate/context/status all work. AUDIT-014's three fixes (write dispatch, cache.lock format, first-use lease) are confirmed.

Two high-severity issues block the coordination workflow:
- **Finding A** (agent ID non-determinism) makes the lease gate unusable without env vars
- **Finding B** (heartbeat CAS failure) means leases cannot be extended

Two medium issues affect data integrity:
- **Finding C** (orphan commits) loses per-document history
- **Finding D** (`link` bypasses git ref) loses relationships on fetch
