---
title: TUI poll uses IssueCache::fetch_all
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- implements: STORY-098
---



## Problem

The TUI background poll thread (event_loop.rs:329-368) reimplements issue fetching inline instead of calling `IssueCache::fetch_all`. The inline implementation silently drops issues that lack a doc ID in the title or a `<!-- lazyspec -->` block in the body. `fetch_all` handles both cases via fallback paths.

## Changes

### Task 1: Replace inline fetch with IssueCache::fetch_all

ACs addressed: Background cache refresh, GitHub Issues appear in document list

Files:
- Modify: `src/tui/infra/event_loop.rs` (the poll thread block, ~lines 329-368)

What to implement:

The `GithubIssuesStore` (held behind `Arc<Mutex<>>` as `shared_store`) already contains `issue_cache: IssueCache`, `issue_map: IssueMap`, `config: Config`, `repo: String`, and `client: GhCli`.

Replace the inline fetch logic (lines 329-368) with:
1. Lock the shared store
2. For each github-issues type_def, call `store.issue_cache.fetch_all(root, type_def, &store.client, &store.repo, &mut store.issue_map, &known_types)`
3. Save the issue_map
4. Release the lock
5. Send `CacheRefresh` event as before

The lock must be held for the `fetch_all` call since it needs mutable access to `issue_map`. This is the same pattern used in `cli/fetch.rs`.

Key constraint: `fetch_all` takes `&dyn GhIssueReader` but `GhCli` is owned by the store. You may need to create a new `GhCli` outside the lock, or restructure the lock scope so the client is borrowed correctly. Check `GhCli::new()` -- it's zero-cost (no state), so creating one per poll is fine.

How to verify:
- `cargo test` passes
- Launch TUI with empty cache, confirm github-issues docs appear after poll cycle
- Issues without doc IDs in titles (e.g. "test", "test2") now appear

## Test Plan

The TUI poll thread spawns real threads, making direct unit testing impractical. Verification is manual:

1. Delete `.lazyspec/cache/testgh/` and `.lazyspec/cache.lock`
2. Launch TUI (`cargo run`)
3. Wait for poll cycle (configured TTL, default 60s)
4. Confirm testgh documents appear in the document list

Existing `IssueCache::fetch_all` tests already cover the fetch logic (fallback paths, issue-number-as-ID, structured and unstructured bodies). This iteration's value is in _reusing_ that tested code path rather than duplicating it.

## Notes

The `GithubIssuesStore` struct already holds `issue_cache` and `issue_map`, so all the pieces are available. The inline code was likely written before `IssueCache` existed or before `fetch_all` was complete.
