---
title: STORY-096 hybrid cache and fetch
type: iteration
status: accepted
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-104
---




## Audit

AUDIT-012 reviewed this iteration against the codebase, STORY-096 ACs, RFC-037, and SOLID principles. This revision addresses all 12 findings.

## Changes

### Task 1: Split `GhClient` into segregated traits

ACs addressed: none (structural improvement, AUDIT-012 Finding 10)

Files:
- Modify: `src/engine/gh.rs`
- Modify: `src/engine/store_dispatch.rs`
- Modify: `src/cli/setup.rs`
- Modify all test mocks that implement `GhClient`

What to implement:

The current `GhClient` trait has 9 methods. Every mock must implement all 9, even when testing a single operation. Split into three traits:

```rust
pub trait GhIssueReader {
    fn issue_list(&self, repo: &str, labels: &[String], json_fields: &[String]) -> Result<Vec<GhIssue>>;
    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue>;
}

pub trait GhIssueWriter {
    fn issue_create(&self, repo: &str, title: &str, body: &str, labels: &[String]) -> Result<GhIssue>;
    fn issue_edit(&self, repo: &str, number: u64, title: Option<&str>, body: Option<&str>, labels_add: &[String], labels_remove: &[String]) -> Result<()>;
    fn issue_close(&self, repo: &str, number: u64) -> Result<()>;
    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()>;
    fn label_create(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()>;
    fn label_ensure(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()>;
}

pub trait GhAuth {
    fn auth_status(&self) -> Result<AuthStatus>;
}
```

`GhCli` implements all three. Update `GithubIssuesStore` generic bounds to `G: GhIssueReader + GhIssueWriter`. Update `setup.rs` to accept `&(dyn GhIssueReader + GhAuth)`. Update test mocks to implement only the traits they need.

How to verify:
```
cargo test
```

---

### Task 2: Consolidate cache writers and issue map usage

ACs addressed: cache-structure-with-timestamps (AUDIT-012 Findings 1, 2, 8)

Files:
- Modify: `src/cli/setup.rs`
- Modify: `src/engine/store_dispatch.rs`

What to implement:

`setup.rs` has two problems: it defines its own `IssueMapEntry` instead of using `engine::issue_map::IssueMap`, and its `write_cache_file` produces non-standard frontmatter (`number`, `title`, `state`, `updated_at`) that `Store::load_with_fs` cannot parse.

1. Delete `setup.rs::IssueMapEntry`. Import and use `engine::issue_map::IssueMap` for all map operations (`load`, `insert`, `save`).

2. Delete `setup.rs::write_cache_file`. Replace with a call to `store_dispatch::write_cache_file`, which produces standard lazyspec frontmatter. This requires parsing each `GhIssue` into a `DocMeta` via `issue_body::deserialize` before writing. The `issue_body::IssueContext` needed for deserialization can be constructed from the `GhIssue` fields.

3. Update `setup.rs::extract_doc_id` to also handle the case where the issue body contains an HTML comment block with frontmatter (the `issue_body` format). When present, extract the ID from the serialized frontmatter rather than parsing the title.

After this task, `lazyspec setup` produces cache files that `Store::load_with_fs` can parse. All cache writes go through a single code path.

How to verify:
```
cargo test setup
cargo test store::github_issues
```

---

### Task 3: `IssueCache` module with `cache.lock` and TTL

ACs addressed: cache-structure-with-timestamps, fresh-cache-hit, stale-cache-triggers-fetch, cold-cache-fetches

Files:
- Create: `src/engine/issue_cache.rs`
- Modify: `src/engine.rs` (add module)

What to implement:

Define an `IssueCache` struct rooted at `.lazyspec/cache/`. Each cached document lives at `.lazyspec/cache/{type}/{id}.md`.

Implement `cache.lock` as a JSON file at `.lazyspec/cache.lock` with per-document timestamps:

```json
{
  "ITERATION-042": { "cached_at": "2026-03-27T10:00:00Z" },
  "STORY-075": { "cached_at": "2026-03-27T09:55:00Z" }
}
```

No ETag field. Conditional refresh is deferred to the native HTTP client.

Methods:

- `IssueCache::new(root: &Path) -> Self`
- `read_lock() -> CacheLock`, `write_lock(&CacheLock)`
- `is_fresh(id: &str, ttl: Duration) -> bool`
- `read_if_fresh(id: &str, doc_type: &str, ttl: Duration) -> Option<String>` (fast path)
- `read_stale(id: &str, doc_type: &str) -> Option<String>` (degradation path)
- `write(id: &str, doc_type: &str, content: &str)` (writes file + updates `cache.lock`)
- `remove(id: &str, doc_type: &str)` (deletes file + removes `cache.lock` entry)
- `list_cached(doc_type: &str) -> Vec<String>` (scans directory for IDs)

The existing `DiskCache` (`src/engine/cache.rs`) caches ref-expanded content keyed by content hash. `IssueCache` caches raw documents keyed by document ID. They coexist.

How to verify:
```
cargo test issue_cache
```

---

### Task 4: Pre-load batch refresh and offline degradation

ACs addressed: fresh-cache-hit, stale-cache-triggers-fetch, cold-cache-fetches, offline-degradation

Files:
- Modify: `src/engine/issue_cache.rs` (add refresh method)

What to implement:

`Store::load_with_fs` loads all documents eagerly by scanning directories. Rather than adding per-document read-through to the store, refresh stale documents _before_ `Store::load` runs.

Add `IssueCache::refresh_stale(type_def: &TypeDef, gh: &dyn GhIssueReader, repo: &str, issue_map: &mut IssueMap, ttl: Duration) -> Result<RefreshResult>`:

1. Check if _any_ cached document of this type is stale via `is_fresh`. If all are fresh, return early (zero API calls).
2. If any are stale, make a single `gh.issue_list` call with `lazyspec:{type}` label filter and `number,title,body,labels,state,updatedAt` fields. This returns all documents for the type with full bodies in one request.
3. For each returned issue, parse via `issue_body::deserialize`, compare against cached content, and call `self.write` for any that changed. Update `issue_map` entries.
4. On API failure: leave all stale cache in place, return `RefreshResult` with a warning. Stale data is better than no data.
5. `RefreshResult` contains `refreshed: usize`, `unchanged: usize`, `warnings: Vec<RefreshWarning>`.

This means the TUI at 60s TTL uses 1 API call per type per TTL cycle, regardless of document count. With 3 github-issues types, that's ~180 calls/hour.

For cold cache (no cached files at all), there's nothing to scan and no staleness to detect. Cold cache is populated by `lazyspec fetch` or `lazyspec setup`.

Callers (CLI commands that load the store) call `refresh_stale` for each github-issues type before `Store::load`. Warnings are printed to stderr. The store itself stays stateless with respect to freshness.

How to verify:
```
cargo test issue_cache::refresh
```

---

### Task 5: `lazyspec fetch` command with pagination and cleanup

ACs addressed: fetch-refreshes-all, fetch-uses-label-filtering, removed-issues-cleaned-up

Files:
- Create: `src/cli/fetch.rs`
- Modify: `src/cli.rs` (register subcommand)
- Modify: `src/main.rs` (wire command)
- Modify: `src/cli/setup.rs` (extract shared fetch logic)

What to implement:

Extract the fetch-and-cache loop from `setup.rs::run` into a shared function in `IssueCache`:

```rust
impl IssueCache {
    pub fn fetch_all(
        &self,
        type_def: &TypeDef,
        gh: &dyn GhIssueReader,
        repo: &str,
        issue_map: &mut IssueMap,
    ) -> Result<FetchResult> { ... }
}
```

`FetchResult` contains `fetched: usize`, `new: usize`, `removed: usize`.

The fetch logic:

1. Call `gh.issue_list` with `lazyspec:{type}` label filter. Handle pagination: `gh issue list` accepts `--limit`; pass a high limit (e.g. 500) to get all results. If the API returns exactly the limit, warn that there may be more issues than retrieved.
2. For each returned issue, parse via `issue_body::deserialize`, write to cache via `IssueCache::write`, update `issue_map`.
3. Compare fetched IDs against `IssueCache::list_cached`. Remove stale entries: delete cache file, remove from `cache.lock`, remove from `issue_map`.

Refactor `setup.rs::run` to call `IssueCache::fetch_all` instead of its inline loop.

Add `lazyspec fetch` subcommand with `--json` and `--type` flags. Calls `fetch_all` for each (or the specified) github-issues type, prints summary.

How to verify:
```
cargo test cli_fetch
cargo test setup
cargo run -- fetch --json
```

---

### Task 6: `GithubIssuesStore` delegates to `IssueCache`

ACs addressed: cache-structure-with-timestamps (AUDIT-012 Finding 9, SRP)

Files:
- Modify: `src/engine/store_dispatch.rs`

What to implement:

`GithubIssuesStore` currently calls `write_cache_file` directly and manages cache paths inline. Replace these with calls to `IssueCache`:

1. Add `issue_cache: IssueCache` field to `GithubIssuesStore`.
2. In `create`: after API call, use `issue_cache.write(id, type_name, content)` instead of inline `std::fs::write`.
3. In `update`: after API call, use `issue_cache.write` instead of `write_cache_file`.
4. In `delete`: use `issue_cache.remove` instead of inline `std::fs::remove_file`.

The standalone `write_cache_file` function in `store_dispatch.rs` remains available for `setup.rs` (Task 2) but `GithubIssuesStore` no longer calls it directly.

This reduces `GithubIssuesStore` to CRUD orchestration: optimistic lock check, API call, delegate cache and map updates.

How to verify:
```
cargo test store_dispatch
```

---

### Task 7: Store integration and end-to-end verification

ACs addressed: fresh-cache-hit, cold-cache-fetches

Files:
- Modify: `src/engine/store.rs` (if needed)
- Modify CLI entry points that load the store (add pre-load refresh calls)

What to implement:

`Store::load_with_fs` already reads from `.lazyspec/cache/{type}/` for github-issues types (line 47-48 of `store.rs`). After Task 2 fixes the cache format, this path works without further changes.

Wire the pre-load refresh (Task 4) into the CLI commands that load the store. Identify the callsites (likely in `src/main.rs` or individual CLI modules) where `Store::load` is called. For each, insert a `refresh_stale` call for github-issues types before loading.

For `list` operations, serve from whatever is cached without triggering refresh. Bulk refresh is `lazyspec fetch`. Per-document staleness refresh only triggers on `show` and `context`.

Verify the full chain works end-to-end: `lazyspec setup` populates cache, `lazyspec list` reads it, `lazyspec show` refreshes stale entries, `lazyspec fetch` does a full refresh with cleanup.

How to verify:
```
cargo test store::github_issues
cargo test -- --test integration  # if integration tests exist
```

## Test Plan

### Test 1: Cache write and fresh read (AC: fresh-cache-hit, cache-structure-with-timestamps)
Write a document to `IssueCache`, immediately read it back with a TTL of 60s. Assert content matches and no API call is made. Verify `.lazyspec/cache/{type}/{id}.md` exists on disk and `cache.lock` contains the entry.

### Test 2: Stale cache returns None from fresh read (AC: stale-cache-triggers-fetch)
Write a document, then set its `cached_at` to 2 minutes ago. Call `read_if_fresh` with a 60s TTL. Assert it returns `None`. Call `read_stale` and assert it returns the content.

### Test 3: Cold cache returns None (AC: cold-cache-fetches)
On a fresh `IssueCache` with no entries, call `read_if_fresh` for a non-existent ID. Assert `None`. Call `read_stale`. Assert `None`.

### Test 4: Cache removal deletes file and lock entry (AC: removed-issues-cleaned-up)
Write two documents. Remove one via `IssueCache::remove`. Assert the removed file is gone, the remaining file is intact, and `cache.lock` has exactly one entry.

### Test 5: Pre-load batch refresh fetches all via `issue_list` (AC: stale-cache-triggers-fetch)
Mock `GhIssueReader`. Set up 3 stale cache entries. Call `refresh_stale`. Assert `issue_list` was called exactly once (not 3 `issue_view` calls). Assert all 3 cache files were updated and `cache.lock` timestamps are fresh.

### Test 6: Pre-load refresh skips API when all fresh (AC: fresh-cache-hit)
Set up 3 fresh cache entries within TTL. Call `refresh_stale`. Assert `issue_list` was never called. Zero API usage on the fast path.

### Test 7: Pre-load refresh returns stale on API failure (AC: offline-degradation)
Mock `GhIssueReader` to return an error on `issue_list`. Set up stale cache entries. Call `refresh_stale`. Assert stale content is unchanged and `RefreshResult.warnings` is non-empty.

### Test 8: Cold cache + API failure returns error (AC: offline-degradation)
No cache entry, API unreachable. Verify that `Store::load` produces an empty set for that type (no panic, no silent failure), and that `lazyspec show` for a missing doc returns a clear error.

### Test 9: Fetch command populates cache with standard frontmatter (AC: fetch-refreshes-all, fetch-uses-label-filtering)
Mock `GhIssueReader::issue_list`. Run `fetch_all`. Assert all returned issues are written to cache with parseable lazyspec frontmatter, `cache.lock` is updated, and `issue-map.json` entries exist. Load the cache directory via `Store::load_with_fs` and assert documents are found.

### Test 10: Fetch command cleans up removed issues (AC: removed-issues-cleaned-up)
Pre-populate cache with 3 documents. Mock `issue_list` returning only 2 of them. Run `fetch_all`. Assert the third document's cache file is deleted and its entries removed from `cache.lock` and `issue-map.json`.

### Test 11: Setup produces parseable cache files (AC: cache-structure-with-timestamps)
Run `setup.rs::run` with mocked `GhIssueReader`. Load the resulting cache directory via `Store::load_with_fs`. Assert documents parse correctly with proper `title`, `type`, `status`, `author`, `date` frontmatter fields.

### Test 12: Segregated trait mocks compile with minimal implementation (structural)
Write a test mock that implements only `GhIssueReader` and verify it compiles and works with `refresh_stale`. Confirms the trait split doesn't force unnecessary stubs.

## Notes

- The existing `DiskCache` (`src/engine/cache.rs`) caches ref-expanded document bodies, keyed by content hash. `IssueCache` caches raw GitHub issue content, keyed by document ID. They coexist.
- `StoreBackend::GithubIssues` and `GithubConfig` already exist in `config.rs`. `cache_ttl` is already a `u64` field with default 60.
- `GhClient` trait, `GhCli` impl, `IssueMap`, and `issue_body` module all exist and are tested. This iteration builds on them.
- ETag/conditional refresh is deferred entirely. When a native HTTP client replaces `gh` CLI, conditional requests can be added without changing the `IssueCache` interface.
- Pagination: `gh issue list --limit 500` is the pragmatic approach. True cursor-based pagination would require `gh api` with Link header parsing, which is deferred to the native HTTP client.
- Rate limits: batch refresh uses `issue_list` (1 call per type) instead of per-document `issue_view`. With 3 github-issues types at 60s TTL, the TUI uses ~180 calls/hour against a 5000/hour budget. CLI commands that don't need refresh (`list`, `search`, `status`) make zero API calls.
