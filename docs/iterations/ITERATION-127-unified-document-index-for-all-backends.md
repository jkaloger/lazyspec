---
title: Unified document index for all backends
type: iteration
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: STORY-099
---



## Summary

`Store::load` currently only reads documents from the filesystem directory configured in `type_def.dir`. For `store = "github-issues"` types, documents live in `.lazyspec/cache/{type_def.name}/` after `lazyspec setup` fetches them. This iteration teaches `Store::load` to also scan cache directories so that github-issues documents appear in the unified index. Once indexed, `show` and `list` work transparently because they already operate on `DocMeta.path` relative to root.

## Task Breakdown

### Task 1: Load cached github-issues documents in Store::load

ACs addressed: Unified document index loads all backends

Files:
- Modify: `src/engine/store.rs` (the `load_with_fs` method)
- Modify: `src/engine/config.rs` (import `StoreBackend` in store.rs)

What to implement:

In `Store::load_with_fs`, after the existing loop over `config.documents.types`, add a second pass for types where `type_def.store == StoreBackend::GithubIssues`. For each such type, construct the cache path as `root.join(".lazyspec/cache").join(&type_def.name)`. If that directory exists, call `loader::load_type_directory` with it (reusing the same `docs`, `children`, `parent_of`, `parse_errors` maps). This means cached `.md` files with valid TOML frontmatter get indexed identically to filesystem documents.

The existing `load_type_directory` call for the type's `dir` should be skipped when `store == GithubIssues`, since those directories may not exist on disk and the canonical source is the cache.

How to verify:
```
cargo test --lib store
```

### Task 2: Add integration tests for unified index loading

ACs addressed: Unified document index loads all backends, Show command works with expanded relationships for all backend types

Files:
- Modify: `src/engine/store.rs` (add tests in the existing `#[cfg(test)]` module)

What to implement:

Add two tests using the existing `InMemoryFileSystem`:

1. `test_load_includes_github_issues_cache`: Configure a type with `store = GithubIssues`, put a valid `.md` file in `.lazyspec/cache/{type_name}/`, verify it appears in `store.docs` and is retrievable via `store.get()`.

2. `test_show_works_for_cached_github_issues_doc`: Same setup, then call `store.get_body_raw()` on the cached doc path and verify the body content is returned.

Both tests should use `InMemoryFileSystem` for isolation and determinism.

How to verify:
```
cargo test --lib store::tests
```

### Task 3: Verify resolve_shorthand works for cached docs

ACs addressed: Show command works with expanded relationships for all backend types

Files:
- Modify: `src/engine/store.rs` (add test in `#[cfg(test)]` module)

What to implement:

Add `test_resolve_shorthand_finds_cached_doc`: Set up a github-issues type with a cached doc like `TESTGH-001-example.md`. Verify `store.resolve_shorthand("TESTGH-001")` returns the correct doc. This confirms the show command's resolution path works for cached documents without any changes to `resolve.rs`.

How to verify:
```
cargo test --lib store::tests::test_resolve_shorthand_finds_cached_doc
```

## Test Plan

All tests use `InMemoryFileSystem` for isolation. No network, no real filesystem, no flakiness.

- `test_load_includes_github_issues_cache`: cached github-issues docs appear in the store's unified index
- `test_show_works_for_cached_github_issues_doc`: `get_body_raw` returns body content for cached docs
- `test_resolve_shorthand_finds_cached_doc`: shorthand resolution works for cached doc IDs

## Notes

The `show` command already reads body via `store.get_body_raw(path, fs)` which joins `root + doc.path`. Cache files use relative paths like `.lazyspec/cache/testgh/TESTGH-001-test.md`, which resolves correctly. No changes needed in `show.rs` or `resolve.rs`.
