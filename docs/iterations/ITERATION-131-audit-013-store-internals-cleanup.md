---
title: AUDIT-013 store internals cleanup
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- related-to: AUDIT-013
- blocks: ITERATION-132
---




## Changes

### Task 1: Replace hand-formatted YAML in `write_cache_file` with serde serialization

Audit finding 10. `write_cache_file` at `src/engine/store_dispatch.rs:329-368` manually constructs YAML frontmatter via `format!()`. Tags containing quotes or YAML-special characters produce invalid YAML. Related fields use hand-built `\n- {}: {}` formatting.

Replace the manual formatting with `serde_yaml::to_string` on a serializable struct (or reuse `DocMeta`'s existing serialization path). The function should produce identical output for common cases, but correctly escape special characters.

Write a test with a tag containing a quote character and a related field with special characters to verify correct escaping.

AC: `write_cache_file` uses serde serialization. Existing tests pass. New test with special characters passes.

### Task 2: Remove `RefCell<IssueMap>` from `GithubIssuesStore`

Audit finding 7. `GithubIssuesStore` at `src/engine/store_dispatch.rs:106` uses `RefCell<IssueMap>` for interior mutability through the `&self` `DocumentStore` trait. This is a runtime borrow check that panics on double-borrow.

Change the `DocumentStore` trait methods to take `&mut self` instead of `&self`. This is safe because:
- `FilesystemStore` doesn't need interior mutability
- `GithubIssuesStore` only uses `RefCell` to work around `&self`
- Callers in `cli/create.rs` and `event_loop.rs` already own the store exclusively

Remove the `RefCell` wrapper. Replace `self.issue_map.borrow()` / `borrow_mut()` with direct `self.issue_map` access. Update all callers of `DocumentStore` methods and `dispatch_for_type` to pass `&mut` references.

AC: No `RefCell<IssueMap>` in `store_dispatch.rs`. `DocumentStore` trait uses `&mut self`. All tests pass.

### Task 3: Converge cache file formatting between `create`, `update`, and `write_cache_file`

Audit finding 6 (partially). After task 1 fixes the YAML formatting, verify that the cache files written during `GithubIssuesStore::create` (line ~174) and `GithubIssuesStore::update` (line ~270) both use `write_cache_file` as the single serialization path. If either method formats cache content inline instead of calling `write_cache_file`, refactor to use the shared function.

AC: `GithubIssuesStore::create` and `update` both call `write_cache_file`. No inline cache formatting remains. Tests pass.

## Test Plan

- `cargo test` passes after each task
- New test: cache file with special characters in tags/related roundtrips correctly
- TUI edit-push still works (manual test: `cargo run -- show` a cached document after create/update)

## Notes

Depends on ITERATION-130 (engine-to-CLI decoupling) because changing `DocumentStore` to `&mut self` is simpler once the trait methods don't call through to CLI code. This iteration blocks ITERATION-132 (TUI async push) because removing `RefCell` changes the store's ownership model, which the TUI refactor needs to account for.
