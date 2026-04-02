---
title: 'STORY-109 Slice 1: Core GitRef storage backend'
type: iteration
status: accepted
author: agent
date: 2026-04-02
tags: []
related:
- implements: STORY-109
---


## Context

This is iteration 1 of 3 for STORY-109 (git-ref storage backend). It delivers the core storage layer: the `StoreBackend::GitRef` config variant, the `GitRefStore<R: GitRefOps>` struct implementing `DocumentStore` (create/update/delete via commit chains under `refs/lazyspec/{type}/{id}`), and the `CacheLock` struct managing `.lazyspec/cache.lock` JSON. Shadow cache files are written alongside each mutation using the existing `write_cache_file()` helper.

This iteration does NOT include `lazyspec fetch`, `Store::load_with_fs` dispatch, `dispatch_for_type` extension, cross-backend reads, or cold cache fallback -- those belong to iterations 2 and 3.

Depends on STORY-108 (accepted), which delivered `GitRefOps` trait, `GitCli`, `MockGitRefClient`, and the lease engine.

## Changes

### Task 1: Add `StoreBackend::GitRef` variant and config parsing

**ACs addressed:** AC Group 1 (StoreBackend::GitRef variant and config parsing)

**Files to modify:**
- `src/engine/config.rs`

**Implementation:**

1. Add a `GitRef` variant to the `StoreBackend` enum at line 106-113 with `#[serde(rename = "git-ref")]` attribute, following the existing `GithubIssues` pattern.
2. Extend the `Display` impl (lines 115-122) to handle `StoreBackend::GitRef => write!(f, "git-ref")`.
3. Add a unit test `test_store_backend_git_ref_parsing` that deserializes `"git-ref"` into `StoreBackend::GitRef` and verifies Display round-trips.
4. Add a unit test `test_store_backend_git_ref_in_config` that loads a full config TOML containing a type with `store = "git-ref"` and verifies `type_def.store == StoreBackend::GitRef`.
5. Add a negative test verifying that a config with no git-ref types still loads correctly and existing backends are unaffected.

**Verification:** `cargo test --lib config` passes. The new variant serializes/deserializes correctly.

---

### Task 2: Implement `CacheLock` struct

**ACs addressed:** AC Group 3 (shadow cache structure and cache.lock)

**Files to modify:**
- `src/engine/store_dispatch.rs` (add `CacheLock` struct near the cache helpers at line 337+)

**Implementation:**

1. Define `CacheLock` struct containing a `BTreeMap<String, String>` mapping doc keys (`{type_name}/{id}`) to ref SHAs.
2. Implement methods:
   - `load(root: &Path) -> Result<CacheLock>`: reads `.lazyspec/cache.lock` JSON. If file does not exist, returns empty map.
   - `save(&self, root: &Path) -> Result<()>`: writes `.lazyspec/cache.lock` as pretty-printed JSON. Creates `.lazyspec/` directory if needed.
   - `get(&self, doc_key: &str) -> Option<&str>`: returns the SHA for a doc key.
   - `set(&mut self, doc_key: &str, sha: &str)`: inserts or updates the entry.
   - `remove(&mut self, doc_key: &str)`: removes the entry.
3. Doc key format is `{type_def.name}/{id}`, e.g. `stories/STORY-109`.
4. Derive `Serialize, Deserialize` for the inner map.

**Verification:** Unit tests for `CacheLock` load/save/get/set/remove using `tempfile::TempDir`. Verify the saved file is valid JSON with expected structure.

---

### Task 3: Implement `GitRefStore<R: GitRefOps>` with `DocumentStore` trait

**ACs addressed:** AC Group 2 (GitRefStore DocumentStore implementation)

**Files to modify:**
- `src/engine/store_dispatch.rs` (add `GitRefStore` struct and `DocumentStore` impl, or create `src/engine/git_ref_store.rs` if the file is too large -- follow the pattern of `GithubIssuesStore` at line 99)

**Implementation:**

1. Define struct:
   ```rust
   pub struct GitRefStore<R: GitRefOps> {
       pub git: R,
       pub root: PathBuf,
       pub config: Config,
   }
   ```

2. Implement `DocumentStore` for `GitRefStore<R>`:

   **`create`:**
   - Generate next ID using the same numbering strategy as `FilesystemStore` (scan existing refs via `git.list_refs(root, "refs/lazyspec/{type_name}/")` to find max ID, increment).
   - Build the document slug from ID and title (reuse existing `slugify` or equivalent).
   - Build markdown content with frontmatter (title, type, status: draft, author, date, tags, related) and the provided body.
   - Call `git.create_ref_commit(root, "refs/lazyspec/{type_name}/{id}", &[("doc.md", &content)])` to create the ref.
   - Get the returned SHA.
   - Call `write_cache_file(root, type_def, &meta, body)` to write `.lazyspec/cache/{type_name}/{id}.md`.
   - Load `CacheLock`, call `set("{type_name}/{id}", &sha)`, save.
   - Return `CreatedDoc { path: cache_path, id }`.

   **`update`:**
   - Load `CacheLock`, get current SHA for `{type_name}/{id}`. Error if not found (document doesn't exist).
   - Read current document from cache file via `find_cache_file`.
   - Parse frontmatter with `DocMeta::parse`, apply field updates from the `updates` slice.
   - Rebuild markdown content.
   - Call `git.create_ref_commit(root, "refs/lazyspec/{type_name}/{id}", &[("doc.md", &new_content)])` to create new commit.
   - Call `git.update_ref(root, refname, &new_sha, &old_sha)` for CAS. If this fails (concurrent modification), return conflict error.
   - Write updated cache file via `write_cache_file`.
   - Update `CacheLock` with new SHA, save.

   **`delete`:**
   - Call `git.delete_ref(root, "refs/lazyspec/{type_name}/{id}")`.
   - Remove cache file at `.lazyspec/cache/{type_name}/{id}.md` (use `find_cache_file` then `fs::remove_file`).
   - Load `CacheLock`, call `remove("{type_name}/{id}")`, save.

**Verification:** See Test Plan below for the full suite.

---

### Task 4: Ensure `.lazyspec/cache/` is gitignored

**ACs addressed:** AC Group 3 (cache directory is gitignored)

**Files to modify:**
- `GitRefStore::create` (or a shared initialization path)

**Implementation:**

1. In `GitRefStore::create` (first cache write), check if `.lazyspec/.gitignore` exists. If not, create it with `cache/\n`. If it exists, check whether `cache/` is already listed; append if not.
2. Alternatively, handle this in `CacheLock::save` since it already creates `.lazyspec/` -- ensure `.lazyspec/.gitignore` includes `cache/`.

**Verification:** Unit test creates a `GitRefStore`, calls `create`, and checks `.lazyspec/.gitignore` contains `cache/`.

## Test Plan

### AC Group 1: StoreBackend::GitRef variant and config parsing

**Test: `test_store_backend_git_ref_serde`**
- Location: `src/engine/config.rs`, `#[cfg(test)] mod tests`
- Asserts: `serde_json::from_str::<StoreBackend>("\"git-ref\"")` yields `StoreBackend::GitRef`; `Display` yields `"git-ref"`.

**Test: `test_config_with_git_ref_type`**
- Location: `src/engine/config.rs`, `#[cfg(test)] mod tests`
- Asserts: A TOML config with `store = "git-ref"` on a type parses successfully, the type has `StoreBackend::GitRef`.

**Test: `test_config_without_git_ref_unaffected`**
- Location: `src/engine/config.rs`, `#[cfg(test)] mod tests`
- Asserts: A TOML config with only `filesystem` and `github-issues` types still parses. No type has `StoreBackend::GitRef`.

### AC Group 2: GitRefStore DocumentStore implementation

All tests use `MockGitRefClient` from `src/engine/git_ref.rs:199-369` and `tempfile::TempDir`.

**Test: `test_git_ref_store_create`**
- Location: `src/engine/store_dispatch.rs` (or `src/engine/git_ref_store.rs`), `#[cfg(test)] mod tests`
- Setup: `MockGitRefClient` configured to succeed on `create_ref_commit`, empty `list_refs`.
- Asserts: `create` returns a `CreatedDoc` with correct ID. Ref was created at `refs/lazyspec/{type}/{id}`. Cache file exists at `.lazyspec/cache/{type}/{id}.md` with correct frontmatter. `cache.lock` contains the doc key mapped to the returned SHA.

**Test: `test_git_ref_store_update`**
- Location: same as above
- Setup: Pre-populate cache file and `cache.lock` with an initial SHA. `MockGitRefClient` succeeds on `create_ref_commit` and `update_ref`.
- Asserts: `update` modifies frontmatter fields. Cache file updated. `cache.lock` SHA updated. `update_ref` called with correct old/new SHAs (CAS).

**Test: `test_git_ref_store_update_cas_conflict`**
- Location: same as above
- Setup: `MockGitRefClient` configured so `update_ref` returns an error (simulating CAS mismatch).
- Asserts: `update` returns an error containing "conflict" or similar. Cache file and `cache.lock` are NOT updated (remain at old values).

**Test: `test_git_ref_store_delete`**
- Location: same as above
- Setup: Pre-populate cache file and `cache.lock`.
- Asserts: `delete` removes the ref (via `delete_ref`). Cache file no longer exists. `cache.lock` no longer contains the doc key.

### AC Group 3: Shadow cache and cache.lock

**Test: `test_cache_lock_round_trip`**
- Location: `src/engine/store_dispatch.rs` (or `src/engine/git_ref_store.rs`), `#[cfg(test)] mod tests`
- Asserts: `CacheLock::load` on empty dir returns empty map. After `set` + `save`, `load` returns the same entries. File is valid JSON. `get` returns correct SHA. `remove` + `save` removes the entry.

**Test: `test_cache_lock_format`**
- Location: same as above
- Asserts: Saved `cache.lock` file is valid JSON. Keys are `{type}/{id}` format. Values are SHA strings.

**Test: `test_gitignore_includes_cache`**
- Location: same as above
- Asserts: After `GitRefStore::create`, `.lazyspec/.gitignore` exists and contains `cache/`.

### Tradeoffs

- Tests use `MockGitRefClient` rather than real git operations. This is correct for unit tests of the storage layer; integration tests with real git repos belong in iteration 2 or 3.
- CAS conflict test relies on `MockGitRefClient` returning an error from `update_ref`. This tests the store's error handling path without needing actual concurrent access.
- The `write_cache_file` helper is already tested elsewhere; these tests focus on the GitRefStore orchestration (that it calls the helper correctly and updates `cache.lock`).

## Notes

- The `create_ref_commit` method in `GitRefOps` takes a `files` slice of `(&str, &str)` pairs. For document storage, each ref commit contains a single file `doc.md`. This is a convention established in this iteration.
- ID generation needs to scan refs, not filesystem directories. `list_refs` with pattern `refs/lazyspec/{type_name}/*` returns existing IDs; parse the max numeric suffix and increment.
- `CacheLock` uses `BTreeMap` for deterministic JSON output (sorted keys), which makes diffs clean and tests stable.
- The CAS semantics on `update_ref` map directly to `git update-ref --stdin` with `verify` directives, which is what `GitCli::update_ref` implements.
