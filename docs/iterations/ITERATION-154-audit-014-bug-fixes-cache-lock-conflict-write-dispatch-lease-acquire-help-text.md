---
title: 'AUDIT-014 bug fixes: cache.lock conflict, write dispatch, lease acquire, help text'
type: iteration
status: accepted
author: agent
date: 2026-04-02
tags: []
related:
- related-to: AUDIT-014
---


## Context

AUDIT-014 found four issues during manual testing of the git-ref storage backend and lease engine. This iteration fixes all four. Standalone (bug fix iteration, no parent story).

Dependency order: Task 1 (cache.lock) must land first since Tasks 2 and 3 interact with cache and ref infrastructure.

## Changes

### Task 1: Unify cache.lock behind a single CacheLock type

**Finding:** AUDIT-014 Finding 2 (high)

**Files to modify:**
- `src/engine/issue_cache.rs` (lines 33-38, 59-74, 98-133) -- remove `CacheLockEntry`, `CacheLock` type alias, `read_lock`, `write_lock`, `touch_lock`; replace with `cache_lock::CacheLock`
- `src/engine/issue_cache.rs` (lines 76-84, `is_fresh`) -- adapt to read a plain string value instead of `entry.cached_at`
- `src/engine/issue_cache.rs` (lines 98-133, `write`, `touch_lock`, `remove`) -- use `CacheLock::load`/`set`/`save` instead of the local read_lock/write_lock methods

**Implementation:**

Both backends store `key -> string` in `.lazyspec/cache.lock`. The only difference is the value: IssueCache stores an RFC3339 timestamp, git-ref stores a SHA. The `CacheLockEntry { cached_at }` wrapper is unnecessary indirection that creates the format conflict.

Remove from `issue_cache.rs`:
- `CacheLockEntry` struct (line 33-36)
- `CacheLock` type alias (line 38)
- `read_lock` method (lines 59-65) -- replace call sites with `cache_lock::CacheLock::load(root)`
- `write_lock` method (lines 67-74) -- replace call sites with `lock.save(root)`
- `lock_path` method (lines 51-53) -- no longer needed

Migrate call sites within `IssueCache`:
- `write` (line 98): `lock.set(id, &Utc::now().to_rfc3339())` instead of inserting a `CacheLockEntry`
- `touch_lock` (line 115): same pattern
- `remove` (line 126): `lock.remove(id)` (already matches)
- `is_fresh` (line 76): `lock.get(id)` returns `Option<&str>`, parse directly as DateTime
- `read_lock` calls in `refresh_stale` and `fetch_all`: replace with `CacheLock::load`

The `IssueCache` constructor stores `root.join(".lazyspec").join("cache")` but needs to pass the project root (not the cache subdir) to `CacheLock::load`. Either store the project root separately, or derive it from `self.root.parent().unwrap()`. The cleanest approach: change `IssueCache` to store the project root directly and compute cache paths from it.

Both backends now share `.lazyspec/cache.lock` with the `BTreeMap<String, String>` schema from `cache_lock.rs`. Keys are namespaced by convention (IssueCache uses doc IDs like `"GH-41"`, git-ref uses `"{type}/{id}"` like `"iteration/ITERATION-042"`), so no collision.

**Existing cache.lock migration:** Existing files have the old `{ "GH-41": { "cached_at": "..." } }` format. After this change, `CacheLock::load` will fail to parse them. Add a one-time migration in `CacheLock::load`: if deserialization fails, try parsing as the old format (`HashMap<String, CacheLockEntry>`), flatten to `BTreeMap<String, String>` by extracting `cached_at` values, and re-save. This avoids breaking existing users.

**Verification:**
- Unit test: write entries from both backends to the same CacheLock, verify both coexist and round-trip
- Unit test: migration test -- write old-format JSON, call CacheLock::load, verify entries are flattened to the new format
- Existing `cache_lock.rs` and `issue_cache.rs` tests pass with the unified type

### Task 2: Wire CLI create/update/delete to GitRefStore

**Finding:** AUDIT-014 Finding 1 (high)

**Files to modify:**
- `src/cli/create.rs` (lines 48-84) -- add `StoreBackend::GitRef` branch before the fs_ops fallthrough
- `src/cli/update.rs` (lines 27-50) -- add `StoreBackend::GitRef` branch
- `src/cli/delete.rs` (lines 25-50) -- add `StoreBackend::GitRef` branch

**Implementation:**

Each CLI mutation function currently has an `if type_def.store == StoreBackend::GithubIssues` branch, then falls through to filesystem operations. Add a matching branch for `StoreBackend::GitRef` that constructs a `GitRefStore { git: GitCli, root, config }` and calls the appropriate `DocumentStore` method.

For `create.rs::run` (line 48): after the GithubIssues block, add:
```rust
if type_def.store == StoreBackend::GitRef {
    let mut store = GitRefStore { git: GitCli, root: root.to_path_buf(), config: config.clone() };
    let created = store.create(type_def, title, author, "")?;
    return Ok(root.join(&created.path));
}
```

Same pattern for `run_json`. For `update.rs` and `delete.rs`, construct the same `GitRefStore` and delegate.

Note: `create.rs` also has `run_json` (around line 87) which needs the same branch. Check both functions.

**Verification:** Integration test using `TestFixture::with_git_remote()`: configure a git-ref type, create a document, verify the git ref exists (`git for-each-ref`), verify cache file exists. Update it, verify ref SHA changed. Delete it, verify ref and cache removed.

### Task 3: Handle missing remote refs in lease acquire

**Finding:** AUDIT-014 Finding 3 (medium)

**Files to modify:**
- `src/engine/lease.rs` (line 60) -- `acquire` method, `fetch_refs` call
- `src/engine/lease.rs` (line 175) -- `force_acquire` method, same issue

**Implementation:**

In `acquire` and `force_acquire`, the `fetch_refs` call uses the exact lease refname. When the ref doesn't exist on the remote, `git fetch` exits with a non-zero status and stderr containing "couldn't find remote ref". This is not a real error -- it means no lease exists yet.

Replace the `?` propagation on `fetch_refs` with error inspection:
```rust
if let Err(e) = self.git.fetch_refs(root, &self.config.remote, &refname) {
    let msg = e.to_string();
    if !msg.contains("couldn't find remote ref") {
        return Err(e);
    }
    // ref doesn't exist on remote -- no existing lease, proceed
}
```

This preserves real network errors while treating "ref not found" as the expected case for first-time claims.

Also check `release` and `heartbeat` -- per the exploration, they don't call `fetch_refs`, so no changes needed there.

**Verification:** Unit test with `MockGitRefClient`: configure `fetch_refs` to return an error containing "couldn't find remote ref", verify `acquire` succeeds and creates the lease. Second test: configure `fetch_refs` to return a network error, verify `acquire` propagates the error.

### Task 4: Update fetch command help text

**Finding:** AUDIT-014 Finding 4 (low)

**Files to modify:**
- `src/cli.rs` (line 220) -- fetch command doc comment

**Implementation:**

Change the doc comment from:
```rust
/// Fetch all github-issues documents from the API
```
to:
```rust
/// Fetch remote documents (github-issues and git-ref types)
```

**Verification:** `cargo run -- help fetch` shows updated text. No test needed.

## Test Plan

| Finding | Test | Type |
|---------|------|------|
| cache.lock unified | Both backends coexist in single CacheLock, entries round-trip | Unit |
| cache.lock migration | Old-format `{ "id": { "cached_at": "..." } }` migrated on load | Unit |
| cache.lock unified | IssueCache is_fresh/write/remove use CacheLock correctly | Unit |
| CLI write dispatch | Create git-ref doc via CLI, verify ref created | Integration |
| CLI write dispatch | Update git-ref doc via CLI, verify ref SHA changed | Integration |
| CLI write dispatch | Delete git-ref doc via CLI, verify ref removed and cache cleaned | Integration |
| Lease acquire | MockGitRefClient returns "couldn't find remote ref" error, acquire succeeds | Unit |
| Lease acquire | MockGitRefClient returns network error, acquire fails | Unit |
| Lease acquire (force) | Same two tests for force_acquire | Unit |
| Help text | Visual inspection of `cargo run -- help fetch` | Manual |

All tests use TempDir, real types (not mocks except for GitRefOps where MockGitRefClient controls I/O), behavioral assertions, deterministic.

The integration tests for Task 2 require `TestFixture::with_git_remote()` since `GitRefStore` calls real git plumbing commands. These are slower (~1-2s each) but necessary for end-to-end verification.

## Notes

- Task 1 unifies the cache.lock schema. Existing `.lazyspec/cache.lock` files with the old `{ "id": { "cached_at": "..." } }` format are migrated on first load. The migration flattens the nested objects by extracting `cached_at` values.
- Task 2 follows the same pattern as the existing GithubIssues branches. A future refactor could replace the if-chain with a single `dispatch_for_type` call, but that's a separate concern (and would touch the GithubIssues path too).
- Task 3 relies on string matching against git's stderr output, which is locale-dependent. The English string "couldn't find remote ref" is stable across git versions. If locale becomes a problem, an alternative is to use `list_refs` with a glob pattern instead of `fetch_refs` with an exact ref.
