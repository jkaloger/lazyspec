---
title: Fetch command and dispatch wiring
type: iteration
status: accepted
author: agent
date: 2026-04-02
tags: []
related:
- implements: STORY-109
---



## Context

Iteration 2 of 3 for STORY-109 (git-ref storage backend). This iteration delivers the `lazyspec fetch` command for git-ref types, the `Store::load_with_fs` dispatch arm for `StoreBackend::GitRef`, and the `dispatch_for_type` extension that routes mutations to `GitRefStore`.

Depends on iteration 1, which delivers `StoreBackend::GitRef` variant, `GitRefStore<R>` implementing `DocumentStore`, `CacheLock` struct, and shadow cache writes.

## Changes

### 1. Extend `lazyspec fetch` for git-ref types

**ACs addressed:** lazyspec fetch command (all three criteria)

**Files:** `src/cli/fetch.rs`

After the existing github-issues fetch block, add a second block that handles `StoreBackend::GitRef` types:

1. Collect config types where `store == StoreBackend::GitRef`, filtered by `type_filter` if provided.
2. For each git-ref type:
   - Call `GitRefOps::fetch_refs(root, remote, "refs/lazyspec/{type_name}/*")` to update local refs from remote.
   - Call `GitRefOps::list_refs(root, "refs/lazyspec/{type_name}/")` to enumerate current local refs after fetch.
   - Load `CacheLock` from `.lazyspec/cache/cache.lock`.
   - For each ref `(refname, sha)` where SHA differs from the cache.lock entry: call `GitRefOps::read_ref_blob(root, sha, "document.md")` to get content, write it to `.lazyspec/cache/{type_name}/{id}.md`, update the cache.lock entry.
   - For each cache.lock entry whose ref no longer exists in the listed refs: delete the cache file and remove the cache.lock entry.
   - Save `CacheLock`.
   - Emit a `TypeSummary` with fetched/new/removed counts.
3. The function signature gains a `git_ref_ops: &dyn GitRefOps` parameter and a `remote: &str` parameter (or reads remote from config). The existing `TypeSummary` struct and JSON/text output already handle the new entries without changes.
4. Early return for "no github-issues types" must be relaxed: the function should proceed if either github-issues or git-ref types exist.

**Verification:** Run `cargo check`. Run new unit tests (see Test Plan).

### 2. Add `StoreBackend::GitRef` arm to `Store::load_with_fs`

**ACs addressed:** Store::load_with_fs dispatch for GitRef

**Files:** `src/engine/store.rs` (lines 46-51)

Extend the path resolution conditional at line 47. Currently it checks `StoreBackend::GithubIssues` and falls through to `root.join(&type_def.dir)`. Change to:

```
StoreBackend::GithubIssues | StoreBackend::GitRef => root.join(".lazyspec/cache").join(&type_def.name),
_ => root.join(&type_def.dir),
```

Both `GithubIssues` and `GitRef` read from the shadow cache directory. No other changes needed; the existing `loader::load_type_directory` handles markdown files the same way regardless of backend.

**Verification:** Run `cargo check`. Existing tests continue to pass. New test (see Test Plan) confirms git-ref type loads from cache dir.

### 3. Extend `dispatch_for_type` with `GitRefStore` parameter

**ACs addressed:** dispatch_for_type extension (both criteria)

**Files:** `src/engine/store_dispatch.rs` (lines 391-407), plus all call sites

1. Add a third parameter `git_ref_store: Option<&'a mut GitRefStore<R>>` to `dispatch_for_type`. The function gains a generic `R: GitRefOps`.
2. Add a `StoreBackend::GitRef` match arm that returns the git-ref store or errors if `None`.
3. Update all call sites to pass the new parameter. Call sites that don't have a `GitRefStore` available pass `None`.

Signature becomes:
```rust
pub fn dispatch_for_type<'a, G: GhIssueReader + GhIssueWriter, R: GitRefOps>(
    type_def: &TypeDef,
    fs_store: &'a mut FilesystemStore,
    gh_store: Option<&'a mut GithubIssuesStore<G>>,
    git_ref_store: Option<&'a mut GitRefStore<R>>,
) -> Result<&'a mut dyn DocumentStore>
```

**Verification:** `cargo check` confirms all call sites compile. Run new unit tests (see Test Plan).

## Test Plan

### AC: fetch with remote git-ref documents (fetch_refs updates cache)

**Test:** `tests/fetch_git_ref.rs` integration test. Set up a `MockGitRefClient` that returns refs with known SHAs. Run the fetch logic. Assert cache files exist at `.lazyspec/cache/{type}/{id}.md` with correct content. Assert `cache.lock` contains the SHA entries.

### AC: fetch with deleted remote document

**Test:** Same integration test file. Pre-populate cache.lock and cache files for a document. Configure `MockGitRefClient::list_refs` to return refs that exclude the deleted document. Run fetch. Assert cache file removed, cache.lock entry removed.

### AC: fetch with no remote git-ref documents

**Test:** Configure `MockGitRefClient::list_refs` to return empty. Run fetch. Assert command succeeds, cache directory empty or absent, cache.lock empty.

### AC: Store::load_with_fs reads from cache for GitRef

**Test:** Unit test in `src/engine/store.rs` `#[cfg(test)]` block. Create a config with a `StoreBackend::GitRef` type. Place a markdown file in `.lazyspec/cache/{type}/`. Call `Store::load_with_fs`. Assert the document is loaded.

### AC: dispatch_for_type routes to GitRefStore

**Test:** Unit test in `src/engine/store_dispatch.rs` `#[cfg(test)]` block. Create a `TypeDef` with `StoreBackend::GitRef`. Call `dispatch_for_type` with `Some(git_ref_store)`. Assert it returns the git-ref store (not the fs or gh store).

### AC: dispatch_for_type ignores GitRefStore for other backends

**Test:** Same test module. Call `dispatch_for_type` with `StoreBackend::Filesystem` and a `Some(git_ref_store)`. Assert it returns the filesystem store. Repeat for `GithubIssues`. Assert git-ref store is not invoked.

## Notes

- **Dependency:** Iteration 1 must land first. This iteration imports `GitRefStore`, `CacheLock`, and `StoreBackend::GitRef` which are all defined there.
- **Remote resolution:** The fetch command needs to know which remote to fetch from. For now, default to `"origin"`. RFC-035 defers configurable remote to Story 3.
- **Blob path convention:** `read_ref_blob` uses path `"document.md"` within the commit tree. This matches the convention established by `GitRefStore::create` in iteration 1.
- **Iteration 3** will handle cross-backend reads (list/show/search/validate/context/status) and cold cache fallback. This iteration only wires up the fetch path and mutation dispatch.
