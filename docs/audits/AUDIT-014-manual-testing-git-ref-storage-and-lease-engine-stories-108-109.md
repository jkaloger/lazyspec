---
title: 'Manual testing: git-ref storage and lease engine (Stories 108/109)'
type: audit
status: draft
author: jack
date: 2026-04-02
tags: []
related:
- related-to: STORY-108
- related-to: STORY-109
- related-to: RFC-035
---

## Scope

End-to-end manual testing of git-ref storage backend (STORY-109) and lease engine (STORY-108) against a real git repository. Audit type: bug bash.

Tested all CLI commands against a document type configured with `store = "git-ref"`: create, update, delete, list, show, search, validate, context, status, fetch, claim, release, heartbeat, leases.

## Criteria

1. CLI write operations (create/update/delete) route to GitRefStore for git-ref types
2. CLI read operations (list/show/search/validate/context/status) include git-ref documents
3. Cache infrastructure (cache.lock, shadow cache) works across backends
4. Lease operations (claim/release/heartbeat) work against a real remote
5. Cold cache fallback materializes documents from refs
6. Fetch materializes cache from remote refs

## Findings

### Finding 1: CLI create/update/delete bypass GitRefStore

**Severity:** high
**Location:** `src/cli/create.rs`, `src/cli/mutate.rs` (update/delete paths)
**Description:** `create.rs` has an explicit branch for `StoreBackend::GithubIssues` that routes to `GithubIssuesStore`, then falls through to `fs_ops::create_document` for all other types, including `GitRef`. The `dispatch_for_type` function in `store_dispatch.rs` correctly routes by backend, but no CLI command calls it for create/update/delete.

Observed behavior:
- `create note "Test" --author jack` writes to `docs/notes/` (filesystem) instead of creating a git ref
- `update` modifies the cache file on disk but does not update the git ref, causing cache/ref divergence
- `delete` removes the cache file but leaves the git ref orphaned

**Recommendation:** Wire CLI create/update/delete to call `dispatch_for_type`, or add explicit `StoreBackend::GitRef` branches matching the existing `GithubIssues` pattern.

### Finding 2: cache.lock format conflict between IssueCache and CacheLock

**Severity:** high
**Location:** `src/engine/issue_cache.rs` (line 38), `src/engine/cache_lock.rs` (line 8)
**Description:** Two modules write `.lazyspec/cache.lock` with incompatible JSON schemas:
- `issue_cache.rs` defines `CacheLock = HashMap<String, CacheLockEntry>` where `CacheLockEntry = { cached_at: String }`
- `cache_lock.rs` defines `CacheLock = BTreeMap<String, String>` (flat key-to-SHA mapping)

When a project has both github-issues and git-ref types, whichever module writes last corrupts the file for the other. `CacheLock::load` from `cache_lock.rs` fails with "invalid type: map, expected a string" when it encounters the `IssueCache` format.

This crashes: `list --json`, `fetch --type <git-ref-type> --json`, `Store::load` cold cache fallback, and any path through `CacheLock::load`.

**Recommendation:** Unify into a single cache.lock schema, or use separate lock files per backend (e.g. `.lazyspec/gitref-cache.lock` for git-ref, keep existing `.lazyspec/cache.lock` for github-issues).

### Finding 3: Lease acquire fails on first use (no remote refs)

**Severity:** medium
**Location:** `src/engine/lease.rs` (line 59-60, `acquire` method)
**Description:** `LeaseEngine::acquire` calls `self.git.fetch_refs(root, &self.config.remote, &refname)` before checking whether a lease exists. When the specific ref does not exist on the remote (which is always the case for a first-time claim), `git fetch` returns a fatal error: "fatal: couldn't find remote ref refs/lazyspec/leases/{type}/{id}".

This makes `lazyspec claim` unusable for any document that has never been claimed before.

**Recommendation:** Treat a "ref not found" fetch failure as "no remote lease exists" rather than a hard error. Either catch the specific error, or fetch with a glob pattern (`refs/lazyspec/leases/{type}/*`) that succeeds even when empty.

### Finding 4: Fetch command help text is outdated

**Severity:** low
**Location:** `src/main.rs` (fetch command description)
**Description:** `lazyspec help fetch` shows "Fetch all github-issues documents from the API" but the command now handles git-ref types too.
**Recommendation:** Update to "Fetch documents from remote backends (github-issues, git-ref)".

### Finding 5: Cold cache fallback works correctly

**Severity:** info
**Location:** `src/engine/store.rs` (`materialize_git_ref_cache`)
**Description:** `Store::load` successfully materialized a git-ref document from refs when no cache directory existed. The document appeared correctly in list, show, search, and context after materialization. Cache file written to `.lazyspec/cache/note/NOTE-001.md` with correct content.

### Finding 6: Read-path commands work correctly for git-ref docs

**Severity:** info
**Location:** `src/cli/` (list, show, search, validate, context, status)
**Description:** All read-path CLI commands correctly include git-ref documents loaded from the shadow cache. No issues found. Documents appear with correct metadata, bodies are readable, search finds matches, validation runs rules, context resolves chains, status includes counts.

## Summary

The read path for git-ref documents is solid: cold cache fallback, list, show, search, validate, context, and status all work correctly.

The write path has two blocking issues: CLI mutations bypass GitRefStore entirely (Finding 1), and the cache.lock format conflict between backends causes crashes when both are active (Finding 2). The lease system cannot acquire a first-time lease due to fetch error handling (Finding 3).

Priority order: Finding 2 (cache.lock conflict) first since it blocks read-path reliability when both backends are present, then Finding 1 (write dispatch), then Finding 3 (lease acquire).
