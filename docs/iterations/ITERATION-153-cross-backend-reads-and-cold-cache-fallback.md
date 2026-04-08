---
title: Cross-backend reads and cold cache fallback
type: iteration
status: accepted
author: agent
date: 2026-04-02
tags: []
related:
- implements: STORY-109
---



## Context

Iteration 3 of 3 for STORY-109 (git-ref storage backend). This slice delivers cross-backend reads and cold cache fallback so that git-ref documents participate in all read-path CLI commands (`list`, `show`, `search`, `validate`, `context`, `status`) and relationship resolution works across backend boundaries.

Depends on:
- Iteration 1 (ITERATION-151): `StoreBackend::GitRef` config variant, `GitRefStore<R>` struct with `DocumentStore` impl, `CacheLock`, shadow cache writes
- Iteration 2 (ITERATION-152): `lazyspec fetch` for git-ref, `Store::load_with_fs` GitRef dispatch, `dispatch_for_type` third parameter

After iterations 1 and 2, `Store::load_with_fs` reads git-ref documents from the shadow cache directory (`.lazyspec/cache/{type_name}`). All downstream commands operate on the loaded `Store`. This iteration adds the cold cache fallback path and verifies that all cross-backend behaviors work end-to-end.

## Changes

### Task 1: Cold cache fallback in `Store::load_with_fs`

**ACs addressed:** Cold cache fallback (both ACs)

**Files to modify:**
- `src/engine/store.rs` (lines 40-81) -- `load_with_fs` signature and body

**Implementation:**

Extend `load_with_fs` to accept an optional `&dyn GitRefOps` parameter. For each `TypeDef` where `store == StoreBackend::GitRef`, after resolving the cache directory path:

1. If the cache directory exists and contains files, proceed with normal `loader::load_type_directory` (the happy path from iteration 2).
2. If the cache directory is empty or missing, and `GitRefOps` is provided, fall back:
   - Call `GitRefOps::list_refs(root, "refs/lazyspec/{type_name}/*")` to enumerate all refs for this type.
   - For each `(refname, sha)` pair, call `GitRefOps::read_ref_blob(root, &sha, "{filename}.md")` to get document content.
   - Write each blob to the cache directory, creating it if needed.
   - Update `CacheLock` with the materialized entries.
   - Then proceed with `loader::load_type_directory` on the now-populated cache.
3. If no `GitRefOps` is provided and cache is empty, skip the type (same as current behavior for missing directories).

All existing call sites pass `None` for the new parameter unless they have a `GitRefOps` available. The main CLI entry point should pass `&GitCli` as the default.

**Verification:** Unit test with `MockGitRefClient` -- set up refs but no cache directory, call `load_with_fs`, confirm documents are loaded and cache files materialized.

### Task 2: Verify `lazyspec list` across backends

**ACs addressed:** Cross-backend reads -- `lazyspec list` shows documents from all three backends

**Files to modify:**
- `tests/` -- new integration test file `tests/cli_git_ref_list_test.rs`

**Implementation:**

No production code changes expected. `list` (in `src/cli/list.rs`) calls `store.all_docs()` which iterates the `Store.docs` HashMap. Since `load_with_fs` already loads git-ref documents into that HashMap (from iteration 2, with cold cache fallback from task 1), `list` works without modification.

Write an integration test that:
- Sets up a `TempDir` with filesystem docs, github-issues cache docs, and git-ref cache docs (or mock refs for cold cache path)
- Runs `lazyspec list --json`
- Asserts documents from all three backends appear in the output

**Verification:** Integration test passes.

### Task 3: Verify `lazyspec show` for git-ref documents

**ACs addressed:** Cross-backend reads -- `lazyspec show {id}` displays git-ref content from shadow cache

**Files to modify:**
- `tests/` -- new integration test file `tests/cli_git_ref_show_test.rs`

**Implementation:**

No production code changes expected. `show` (in `src/cli/show.rs`) resolves a document by ID from the `Store` and reads its content from disk. Since git-ref documents are cached to `.lazyspec/cache/{type_name}/`, the file read works as-is.

Write an integration test that:
- Sets up a `TempDir` with a git-ref document in the cache directory
- Runs `lazyspec show {id} --json`
- Asserts the content is displayed correctly

**Verification:** Integration test passes.

### Task 4: Verify `lazyspec search` includes git-ref documents

**ACs addressed:** Cross-backend reads -- `lazyspec search {query}` returns git-ref matches

**Files to modify:**
- `tests/` -- new integration test file `tests/cli_git_ref_search_test.rs`

**Implementation:**

No production code changes expected. `search` (in `src/cli/search.rs`) operates on the loaded `Store`. Write an integration test that:
- Sets up a `TempDir` with a git-ref document containing a known search term
- Runs `lazyspec search {term} --json`
- Asserts the git-ref document appears in results

**Verification:** Integration test passes.

### Task 5: Verify `lazyspec validate` covers git-ref documents

**ACs addressed:** Cross-backend reads -- `lazyspec validate` validates git-ref documents alongside others

**Files to modify:**
- `tests/` -- new integration test file `tests/cli_git_ref_validate_test.rs`

**Implementation:**

No production code changes expected. `validate` (in `src/cli/validate.rs`) operates on the loaded `Store`. Write an integration test that:
- Sets up a `TempDir` with a git-ref document that has a validation issue (e.g., missing required field)
- Runs `lazyspec validate --json`
- Asserts the validation error for the git-ref document is reported

**Verification:** Integration test passes.

### Task 6: Cross-backend relationship resolution and `lazyspec context`

**ACs addressed:** Cross-backend reads -- `lazyspec context {id}` resolves full chain; Cross-backend relationship resolution (both ACs)

**Files to modify:**
- `src/cli/context.rs` -- potentially no changes needed
- `src/engine/store/links.rs` -- potentially no changes needed
- `tests/` -- new integration test file `tests/cli_git_ref_context_test.rs`

**Implementation:**

Relationship resolution in `links.rs` uses `resolve_target` which maps document IDs to paths via the `id_to_path` HashMap. Since git-ref documents are loaded into the Store with their cache paths, and `related` frontmatter uses IDs (e.g., `implements: STORY-109`), cross-backend links should resolve without changes -- the ID-based lookup is backend-agnostic.

`resolve_chain` in `context.rs` follows `Implements` relationships by looking up `store.get(&PathBuf::from(&rel.target))`. Since `build_links` has already resolved IDs to paths and stored them in `forward_links`/`reverse_links`, this should work across backends.

Investigate whether any path assumptions exist that would break. If `resolve_target` or `store.get` makes assumptions about path structure (e.g., assuming docs live under `docs/`), that would need fixing.

Write an integration test that:
- Sets up a filesystem story and a git-ref iteration that `implements` the story
- Runs `lazyspec context {iteration_id} --json`
- Asserts the chain includes both the story (filesystem) and iteration (git-ref)
- Also test the reverse: `lazyspec context {story_id} --json` shows the git-ref iteration as a forward dependency

**Verification:** Integration test passes with cross-backend chain resolution.

### Task 7: Verify `lazyspec status` includes git-ref documents

**ACs addressed:** Cross-backend reads -- `lazyspec status` includes git-ref documents

**Files to modify:**
- `tests/` -- new integration test file `tests/cli_git_ref_status_test.rs`

**Implementation:**

No production code changes expected. `status` (in `src/cli/status.rs`) aggregates document counts and statuses from the loaded `Store`. Write an integration test that:
- Sets up a `TempDir` with filesystem docs and git-ref cache docs
- Runs `lazyspec status --json`
- Asserts git-ref document counts are included in the summary

**Verification:** Integration test passes.

### Task 8: Bidirectional cross-backend relationship smoke test

**ACs addressed:** Cross-backend relationship resolution -- both directions

**Files to modify:**
- `tests/` -- can be combined into `tests/cli_git_ref_context_test.rs` from task 6

**Implementation:**

Extend the task 6 test to cover both directions:
- Filesystem doc with `related: [{implements: GIT-REF-DOC}]` -- verify the git-ref doc is accessible via relationship resolution
- Git-ref doc with `related: [{implements: FS-DOC}]` -- verify the filesystem doc is accessible

If `resolve_target` in `links.rs` has issues resolving IDs that map to cache paths, fix the resolution logic. The current implementation looks correct since it uses `id_to_path` which is built from all loaded docs regardless of backend.

**Verification:** Integration tests pass in both directions.

## Test Plan

| AC | Test | Type |
|---|---|---|
| `list` shows all backends | Set up 3-backend store, run `list --json`, assert all present | Integration |
| `show` displays git-ref from cache | Set up cached git-ref doc, run `show --json`, assert content | Integration |
| `search` includes git-ref | Set up git-ref doc with search term, run `search --json`, assert hit | Integration |
| `validate` covers git-ref | Set up git-ref doc with validation error, run `validate --json`, assert error reported | Integration |
| `context` resolves cross-backend chain | Set up fs-story + git-ref-iteration chain, run `context --json`, assert full chain | Integration |
| `status` includes git-ref | Set up mixed store, run `status --json`, assert git-ref counts | Integration |
| Cross-backend relationship (git-ref -> fs) | Git-ref doc implements fs doc, resolve both directions | Integration |
| Cross-backend relationship (fs -> git-ref) | Fs doc implements git-ref doc, resolve both directions | Integration |
| Cold cache fallback | MockGitRefClient with refs, empty cache dir, load store, assert docs loaded and cache materialized | Unit |
| Empty cache + no GitRefOps | No refs, no cache, load store, assert type skipped gracefully | Unit |

All tests use `tempfile::TempDir`, real types (not mocks) except for `GitRefOps` where `MockGitRefClient` controls I/O, behavioral assertions, and are deterministic.

## Notes

- The only production code change expected is in task 1 (cold cache fallback in `load_with_fs`). Tasks 2-8 are primarily verification and integration testing. If any of those tasks reveal that cross-backend reads don't work out of the box, the fix will be scoped to the specific module that breaks.
- The `resolve_target` function in `links.rs` is backend-agnostic (ID-based lookup), so cross-backend links should work without modification.
- The cold cache fallback adds a `GitRefOps` dependency to `load_with_fs`. This is passed as an `Option<&dyn GitRefOps>` to avoid breaking existing call sites. The main CLI entry point passes `&GitCli`; tests pass `&MockGitRefClient`.
- Risk: if iteration 1 or 2 change the cache path structure or the way git-ref documents are loaded, task 1 may need adjustment. Coordinate on the cache directory convention (`.lazyspec/cache/{type_name}/`).
