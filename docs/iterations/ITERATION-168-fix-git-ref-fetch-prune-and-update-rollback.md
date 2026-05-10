---
title: "Fix git-ref fetch prune and update rollback"
type: iteration
status: accepted
author: "claude"
date: 2026-05-09
tags: []
related: []
---

## Context

Git-ref store audit (2026-05-09) found two concurrency defects affecting multi-clone coordination:

1. `fetch_refs` in `src/engine/git_ref.rs:203-211` runs `git fetch +pat:pat` without `--prune`. Remote-deleted doc/lease refs leave ghost local refs and stale cache files. `cli/fetch.rs:fetch_git_ref_type` lists local refs after fetch, so deletion is invisible. Same shape in `cli/lease.rs:87` for lease refs.

2. `GitRefStore::update`, `set_provenance`, `delete` in `src/engine/git_ref_store.rs` call local `update_ref` (CAS) before `push_ref`. On push rejection (non-FF), local ref already advanced but cache.lock and cache file remain at old SHA. Subsequent updates fail with `cannot lock ref ... is at X but expected Y` until manual `lazyspec fetch` force-overwrites the local ref.

Both defects reproduced in audit with bare-repo + 2-clone scenario. CLI and TUI share engine path so both surface the bugs.

## Acceptance Criteria

AC1. After `lazyspec delete` from clone A and `lazyspec fetch` from clone B, B's local doc ref `refs/lazyspec/{type}/{id}` and cache file `.lazyspec/cache/{type}/{id}.md` no longer exist.

AC2. After `lazyspec release` from clone A and `lazyspec claim` from clone B, B sees no stale local lease ref and the claim succeeds without manual ref deletion.

AC3. When `GitRefStore::update` push fails (non-FF rejection), local ref `refs/lazyspec/{type}/{id}` remains at old SHA. Subsequent `lazyspec fetch` + retry succeeds without `cannot lock ref` error.

AC4. AC3 holds for `set_provenance` flow as well. `delete` flow: if remote `delete_remote_ref` fails, local ref stays intact (already current behavior; verify with test).

## Changes

1. **Add `--prune` to `GitCli::fetch_refs`** (`src/engine/git_ref.rs:203-211`).
   - ACs: AC1, AC2.
   - Insert `"--prune"` into the args slice passed to `git fetch`. Refspec stays `+pat:pat`.
   - Verify: `cargo test fetch` and the new integration test (Test 1).

2. **Rollback local ref on push failure in `GitRefStore::update`** (`src/engine/git_ref_store.rs:152-219`).
   - ACs: AC3.
   - Wrap the `push_ref` call: on `Err(push_err)`, run `update_ref(refname, old_sha, new_sha)` (reverse CAS: expect current new_sha, set back to old_sha). Then `bail!` with the original push error.
   - If rollback itself errors, surface a combined error mentioning that local state is wedged and recovery requires `lazyspec fetch`.
   - Cache file write and cache.lock save stay after successful push (already correct).

3. **Apply same rollback to `set_provenance`** (`src/engine/git_ref_store.rs:221-288`).
   - ACs: AC4.
   - Identical pattern to Change 2.

4. **Verify `delete` flow safety** (`src/engine/git_ref_store.rs:290-308`).
   - ACs: AC4 (delete portion).
   - Current order: `delete_remote_ref` -> `delete_ref` (local) -> remove cache file -> update cache.lock. If remote delete fails, code bails before local delete (safe). If local `delete_ref` fails after remote succeeded, cache file and lock are not removed (safe; recovery: re-fetch will recreate). No code change required; add a regression test confirming this order.

## Test Plan

Test framework: `tests/` integration tests using `tempfile::TempDir` + bare git repo + two working clones for concurrency tests; `MockGitRefClient` for unit tests. Per dictum-004 (testing): isolated, deterministic, no sleeps, real types over mocks at trait seams, behavioral assertions.

**Test 1: fetch prunes deleted remote refs** (AC1) — integration.
Setup: bare repo, clone A and B; both init lazyspec config with git-ref iteration type and `[coordination]`. A claims+creates ITERATION-001 (pushes ref). B fetches; assert B has local ref + cache file. A claims+deletes ITERATION-001. B fetches.
Assert: B has no `refs/lazyspec/iteration/ITERATION-001` ref AND no `.lazyspec/cache/iteration/ITERATION-001.md`. `fetch_git_ref_type` returns `removed: 1`.

**Test 2: fetch prunes deleted remote lease refs** (AC2) — integration.
Setup: same. A claims ITERATION-001 (lease pushed). B fetches leases via lease module path. A releases. B claims ITERATION-001.
Assert: claim succeeds (no "lease held" error). No manual `git update-ref -d` needed.

**Test 3: update rollback on push rejection** (AC3) — unit, using `MockGitRefClient`.
Configure: `create_commit_results = Ok("newsha")`, `update_ref_results = [Ok(()), Ok(())]` (first for forward CAS, second for rollback), `push_results = Err("non-fast-forward")`. Pre-seed cache.lock with old_sha and cache file with status=draft.
Assert: `update` returns Err. Mock `calls` includes two `update_ref` entries: forward `(refname, newsha, oldsha)` then rollback `(refname, oldsha, newsha)`. Cache file content unchanged (status=draft). Cache.lock unchanged (oldsha).

**Test 4: set_provenance rollback on push failure** (AC4) — unit, using `MockGitRefClient`.
Same shape as Test 3 but invoking `set_provenance`. Same assertions.

**Test 5: delete preserves local state when remote delete fails** (AC4) — unit, using `MockGitRefClient`.
Configure: `delete_remote_results = Err("network down")`. Pre-seed cache file and cache.lock.
Assert: `delete` returns Err. Cache file still exists. Cache.lock entry still present. Local doc ref not deleted (`delete_ref_results` queue untouched).

**Test 6: end-to-end rollback recovery** (AC3) — integration.
Setup: bare repo, A and B seeded with ITERATION-001 status=draft (B's clone fetched). A updates status=accepted (push succeeds). B (stale) updates status=review.
Assert: B's update bails. B's local ref equals B's cache.lock SHA (the pre-attempt SHA). B's cache file content unchanged.
Then: B runs `lazyspec fetch`. B retries update status=review. Push succeeds. Remote ref now at B's commit chained on A's accepted.

Tradeoffs:
- Tests 1, 2, 6 use real `git` subprocess. Slower but exercise the actual `--prune` flag and end-to-end CAS semantics. Worth it: `git fetch --prune` behavior is exactly what AC1 promises; a mock can't validate the subprocess argument set.
- Tests 3, 4, 5 are fast unit coverage decoupled from git subprocess; they assert the rollback control flow without needing a real remote.

## Notes

- Audit reproduction captured 2026-05-09 in conversation transcript. Bare repo at `/tmp/lzs2/remote.git` during audit (not retained).
- Rollback uses reverse-CAS `update_ref(refname, old_sha, new_sha)`: expect new_sha (current local after forward CAS), set back to old_sha. Standard git operation.
- Skill audit (out of scope here) flagged that skills don't reference fetch/claim/release lifecycle when coordination is configured. Tracked separately.
- TUI in-place edit (`src/tui/infra/event_loop.rs:try_push_git_ref_edit`) calls `GitRefStore::update` and inherits the fix automatically.
