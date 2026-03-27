---
title: STORY-096 hybrid cache and fetch
type: iteration
status: draft
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-096
---


## Changes

### Task 1: Cache directory structure and cache.lock

**ACs addressed:** cache-structure-with-timestamps, fresh-cache-hit

**Files:**
- Create: `src/engine/issue_cache.rs`
- Modify: `src/engine.rs` (add module)

**What to implement:**

Define an `IssueCache` struct rooted at `.lazyspec/cache/`. Each cached document lives at `.lazyspec/cache/{type}/{id}.md`, matching the layout from RFC-037.

Implement `cache.lock` as a JSON file at `.lazyspec/cache.lock` containing per-document timestamps and optional ETags:

```json
{
  "ITERATION-042": { "cached_at": "2026-03-27T10:00:00Z", "etag": "W/\"abc123\"" },
  "STORY-075": { "cached_at": "2026-03-27T09:55:00Z", "etag": null }
}
```

Provide methods: `read_lock()`, `write_lock()`, `entry_for(id)`, `update_entry(id, timestamp, etag)`, `remove_entry(id)`. Use `serde_json` for serialization. Create directories lazily on first write.

The existing `DiskCache` in `src/engine/cache.rs` is a ref-expansion cache (keyed by path hash + body hash). This new cache is structurally different and lives in a separate module.

**How to verify:**
```
cargo test issue_cache
```

---

### Task 2: TTL freshness checking

**ACs addressed:** fresh-cache-hit, stale-cache-triggers-fetch

**Files:**
- Modify: `src/engine/issue_cache.rs`
- Modify: `src/engine/config.rs` (add `cache_ttl` to github config)

**What to implement:**

Add a `cache_ttl` field to the `[github]` config section (default `60s`, parsed as `Duration`). If no `[github]` section exists yet in config, add the struct.

Add `IssueCache::is_fresh(id: &str, ttl: Duration) -> bool` that reads the `cached_at` timestamp from `cache.lock`, compares against `now`, and returns whether the entry is within TTL.

Add `IssueCache::read_if_fresh(id: &str, doc_type: &str, ttl: Duration) -> Option<String>` that returns cached content only if fresh. This is the fast read path -- no API call needed.

Add `IssueCache::read_stale(id: &str, doc_type: &str) -> Option<String>` for the degradation path -- returns content regardless of TTL.

**How to verify:**
```
cargo test issue_cache::freshness
```

---

### Task 3: Cache write path

**ACs addressed:** cold-cache-fetches, stale-cache-triggers-fetch, cache-structure-with-timestamps

**Files:**
- Modify: `src/engine/issue_cache.rs`

**What to implement:**

Add `IssueCache::write(id: &str, doc_type: &str, content: &str, etag: Option<&str>)` that:

1. Writes the document content to `.lazyspec/cache/{type}/{id}.md`
2. Updates `cache.lock` with the current timestamp and optional ETag
3. Creates the type subdirectory if it does not exist

Add `IssueCache::remove(id: &str, doc_type: &str)` for cleanup of deleted issues. Removes the cache file and the `cache.lock` entry.

Add `IssueCache::list_cached(doc_type: &str) -> Vec<String>` that returns all cached document IDs for a given type by scanning the directory.

**How to verify:**
```
cargo test issue_cache::write
```

---

### Task 4: Cache-aware read path

**ACs addressed:** fresh-cache-hit, stale-cache-triggers-fetch, cold-cache-fetches, offline-degradation

**Files:**
- Create: `src/engine/github_read.rs` (or integrate into an existing github module if one exists)
- Modify: `src/engine.rs`

**What to implement:**

Implement the read-through logic described in RFC-037:

1. If `IssueCache::read_if_fresh` returns content, return it (fast path, no API call)
2. If stale or missing, call `gh issue view {number} --json body,title,labels,state,updatedAt` via the gh CLI layer
3. On success: write to cache via `IssueCache::write`, return fresh content
4. On API failure: call `IssueCache::read_stale`. If stale content exists, return it with a warning on stderr. If no cache exists at all, return an error.

The gh CLI integration layer (STORY-095 / ITERATION-121) provides the shell-out mechanism. If that layer does not exist yet, define a trait `GhClient` with a `view_issue(number: u64) -> Result<IssueData>` method and a stub implementation. The real implementation will be wired in when ITERATION-121 lands.

**How to verify:**
```
cargo test github_read
```

---

### Task 5: Conditional refresh with ETag/If-Modified-Since

**ACs addressed:** stale-cache-triggers-fetch (efficiently)

**Files:**
- Modify: `src/engine/github_read.rs`
- Modify: `src/engine/issue_cache.rs` (ETag storage already in Task 1)

**What to implement:**

When refreshing a stale cache entry, pass the stored ETag to the gh CLI call. The `gh api` command supports custom headers:

```
gh api repos/{owner}/{repo}/issues/{number} \
  -H "If-None-Match: W/\"abc123\""
```

If the response is 304 Not Modified, update only the `cached_at` timestamp in `cache.lock` (extending freshness) without rewriting the cache file. If 200, write the new content and ETag as usual.

This reduces bandwidth and counts toward GitHub's rate limit at a lower cost. If the gh CLI layer does not expose header control, fall back to unconditional fetch and note the limitation.

**How to verify:**
```
cargo test github_read::conditional
```

---

### Task 6: `lazyspec fetch` command

**ACs addressed:** fetch-refreshes-all, fetch-uses-label-filtering, removed-issues-cleaned-up

**Files:**
- Create: `src/cli/fetch.rs`
- Modify: `src/cli.rs` (register subcommand)
- Modify: `src/main.rs` (wire command)

**What to implement:**

Add a `lazyspec fetch` subcommand that:

1. For each type with `store = "github-issues"`, call `gh issue list --label "lazyspec:{type}" --json number,title,body,labels,state,updatedAt --limit 100` with pagination (repeat with `--page` until results are empty or use `gh api` with pagination).
2. For each returned issue, write to cache via `IssueCache::write` and update `issue-map.json`.
3. Compare the set of fetched IDs against `IssueCache::list_cached(type)`. Any cached ID not in the fetched set represents a removed issue: delete its cache file and remove its `cache.lock` and `issue-map.json` entries.
4. Print a summary: `Fetched 12 iterations (2 new, 1 removed)`.

Support `--json` flag for machine-readable output. Support `--type` flag to limit fetch to a single document type.

**How to verify:**
```
cargo test cli_fetch
cargo run -- fetch --json
```

---

### Task 7: Integration with store read path

**ACs addressed:** fresh-cache-hit, cold-cache-fetches

**Files:**
- Modify: `src/engine/store/loader.rs`
- Modify: `src/engine/store.rs`

**What to implement:**

In `Store::load_with_fs`, when processing a `TypeDef` with `store = "github-issues"`, load documents from the cache directory (`.lazyspec/cache/{type}/`) instead of the configured `dir`. The cache files have standard frontmatter, so the existing `load_type_directory` works on them.

This means `lazyspec list`, `lazyspec show`, `lazyspec search` all read from cache transparently. The fetch/refresh logic in Task 4 is invoked on the read path when a specific document is requested and its cache is stale.

For `list` operations, serve from whatever is cached. Staleness-triggered refresh only applies to single-document reads (`show`, `context`). Bulk refresh is `lazyspec fetch`.

**How to verify:**
```
cargo test store::github_issues
```

## Test Plan

### Test 1: Cache write and fresh read (AC: fresh-cache-hit, cache-structure-with-timestamps)
Write a document to `IssueCache`, immediately read it back with a TTL of 60s. Assert content matches and no API call is made. Verify `.lazyspec/cache/{type}/{id}.md` exists on disk and `cache.lock` contains the entry. Isolated, fast, filesystem-only.

### Test 2: Stale cache returns None from fresh read (AC: stale-cache-triggers-fetch)
Write a document, then set its `cached_at` to 2 minutes ago. Call `read_if_fresh` with a 60s TTL. Assert it returns `None`. Call `read_stale` and assert it returns the content. Isolated, fast, deterministic (manual timestamp).

### Test 3: Cold cache returns None (AC: cold-cache-fetches)
On a fresh `IssueCache` with no entries, call `read_if_fresh` for a non-existent ID. Assert `None`. Call `read_stale`. Assert `None`. Isolated, fast.

### Test 4: Cache removal deletes file and lock entry (AC: removed-issues-cleaned-up)
Write two documents. Remove one via `IssueCache::remove`. Assert the removed file is gone, the remaining file is intact, and `cache.lock` has exactly one entry. Isolated, fast.

### Test 5: Read-through fetches on stale cache (AC: stale-cache-triggers-fetch)
Mock the `GhClient` trait. Set up a stale cache entry. Call the read-through function. Assert the mock was called exactly once, the cache file was updated, and `cache.lock` timestamp is fresh. Isolated, fast, no real API calls.

### Test 6: Read-through returns stale on API failure (AC: offline-degradation)
Mock the `GhClient` to return an error. Set up a stale cache entry. Call read-through. Assert stale content is returned and a warning is emitted to stderr. Isolated, fast.

### Test 7: Read-through fails on cold cache + API failure (AC: offline-degradation)
Mock the `GhClient` to return an error. No cache entry. Call read-through. Assert an error is returned (not a silent empty response). Isolated, fast.

### Test 8: Fetch command populates cache (AC: fetch-refreshes-all, fetch-uses-label-filtering)
Mock `gh issue list` output. Run `lazyspec fetch`. Assert all returned issues are written to cache, `cache.lock` is updated, and `issue-map.json` entries exist. Isolated, fast.

### Test 9: Fetch command cleans up removed issues (AC: removed-issues-cleaned-up)
Pre-populate cache with 3 documents. Mock `gh issue list` returning only 2 of them. Run fetch. Assert the third document's cache file is deleted and its entries removed from `cache.lock` and `issue-map.json`. Isolated, fast.

### Test 10: ETag conditional refresh skips body write on 304 (AC: stale-cache-triggers-fetch)
Mock `GhClient` to return a 304 response. Set up a stale cache entry with an ETag. Call read-through. Assert the cache file content is unchanged, but `cached_at` in `cache.lock` is updated. Isolated, fast.

## Notes

- The existing `DiskCache` (`src/engine/cache.rs`) caches ref-expanded document bodies, keyed by content hash. The new `IssueCache` caches raw GitHub issue content, keyed by document ID. They serve different purposes and coexist.
- `store = "github-issues"` on `TypeDef` does not exist in the config schema yet. Task 7 assumes it will be added (likely by STORY-095's iteration or a config-focused story). If not present, gate the cache integration behind a check for the field.
- The `GhClient` trait in Task 4 is a seam for testing. The real implementation depends on ITERATION-121 (gh CLI integration layer). If that iteration lands first, wire directly into it instead of creating a separate trait.
- `issue-map.json` is managed by STORY-095 (Issue CRUD). This iteration reads and writes to it during fetch (Task 6) but does not own its schema. Coordinate with ITERATION-120.
