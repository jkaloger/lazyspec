---
title: Cross-backend validation and context
type: iteration
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: STORY-099
---

## Changes

### Task 1: Extend Store::load to include github-issues cache documents

ACs: cross-backend-relationship-resolution

Files:
- `src/engine/store.rs` (modify `load_with_fs` to dispatch per TypeDef backend)
- `src/engine/store/loader.rs` (add cache-directory loading for github-issues types)
- `src/engine/config.rs` (reference only; `StoreBackend` enum already exists)

`Store::load_with_fs` currently loads every TypeDef from its configured `dir`. For `GithubIssues` types, also scan `.lazyspec/cache/{type}/` and merge those documents into `store.docs`. This is the prerequisite from Iteration 1 (ITERATION-122 Task 2), but if it lands as part of that iteration, this task becomes a no-op verification. If not, implement the cache-directory scan here.

After this task, `store.docs` contains documents from both filesystem and github-issues backends, keyed by relative path. The `id_to_path` map in `build_links` spans all backends automatically.

Verify: `cargo test store`

---

### Task 2: Make resolve_shorthand work for cache-path documents

ACs: cross-backend-relationship-resolution

Files:
- `src/engine/store.rs` (`resolve_shorthand`, `resolve_unqualified`, `canonical_name`)

`canonical_name` extracts from filesystem-style paths. Documents loaded from `.lazyspec/cache/` have paths like `.lazyspec/cache/iteration/ITERATION-042-foo.md`. The canonical_name derivation works because it reads the filename stem, but verify this explicitly. Add a fallback in `resolve_unqualified` that matches on the doc's `id` field if `canonical_name` fails.

Verify: `cargo test store -- resolve`

---

### Task 3: Context command renders backend type in chain output

ACs: context-follows-cross-backend-chains

Files:
- `src/cli/context.rs` (`mini_card`, `run_human`, `run_json`)
- `src/cli/json.rs` (`doc_to_json_with_family`)
- `src/engine/config.rs` (lookup: TypeDef for a given DocType)

Add a `backend` annotation to each document in the context chain. In `run_human`, append `[filesystem]` or `[github-issues]` to the `mini_card` second line. In `run_json`, add a `"backend"` field to each chain entry.

The backend is determined by matching `DocMeta.doc_type` against the config's TypeDefs and reading the `store` field. Pass `Config` into `run_human`/`run_json` so it can look up backend provenance.

Verify: `cargo test context`

---

### Task 4: Validate detects broken cross-backend references

ACs: validate-detects-broken-cross-backend-relationships

Files:
- `src/engine/validation.rs` (`BrokenLinkRule::check`)

`BrokenLinkRule` builds an `id_to_path` map from `store.docs`. If Task 1 includes cache documents in the index, broken link detection already works cross-backend. Improve diagnostics: when a broken link target matches a configured type with `StoreBackend::GithubIssues`, include a hint suggesting the user run `lazyspec setup` to refresh the cache.

Verify: `cargo test validation`

---

### Task 5: Integration tests for cross-backend chains

ACs: all three ACs

Files:
- `tests/cross_backend_context_test.rs` (create)

Build an integration test using `InMemoryFileSystem`:

1. Create a config with an RFC type (filesystem) and an iteration type (github-issues)
2. Populate filesystem RFC and cache-directory iteration that `implements` the RFC
3. Load the Store and assert `resolve_chain` returns full chain
4. Assert `validate_full` reports no errors for valid cross-backend links
5. Remove the RFC, reload, assert `BrokenLink` error on the iteration
6. Assert `run_json` output includes backend annotations

Verify: `cargo test cross_backend_context`

## Test Plan

### Test 1: resolve_shorthand finds cache-path documents (AC: cross-backend-relationship-resolution)
Create a github-issues doc in `.lazyspec/cache/iteration/ITERATION-042-foo.md`. Call `resolve_shorthand("ITERATION-042")`. Assert it returns the document.

### Test 2: forward_links cross backend boundaries (AC: cross-backend-relationship-resolution)
Create an iteration (cache) implementing a story (filesystem). Assert `forward_links_for` on the iteration returns the story path, and `reverse_links_for` on the story returns the iteration path.

### Test 3: Context chain includes backend annotation in JSON (AC: context-follows-cross-backend-chains)
Build RFC -> Story -> Iteration across backends. Run `run_json`. Assert each chain entry has `"backend"` with correct value.

### Test 4: Context chain human output shows backend label (AC: context-follows-cross-backend-chains)
Same chain. Run `run_human`. Assert output contains `[github-issues]` for the iteration card.

### Test 5: Broken cross-backend link produces validation error (AC: validate-detects-broken-cross-backend-relationships)
Create iteration with `implements: STORY-999` (nonexistent). Run `validate_full`. Assert `BrokenLink` error.

### Test 6: Valid cross-backend link passes validation (AC: validate-detects-broken-cross-backend-relationships)
Create a linked pair across backends. Run `validate_full`. Assert no `BrokenLink` errors.

## Notes

This iteration depends on ITERATION-122 completing first. If the unified index is already in place after Iteration 1, Tasks 1-2 reduce to verification and edge-case hardening. The substantive new work is in Tasks 3-5: backend annotations in context output and improved validation diagnostics.

The `Config` needs to be threaded into the context rendering functions, which currently only receive a `Store`. This is the main structural change.
