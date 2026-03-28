---
title: AUDIT-013 duplication cleanup
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- related-to: AUDIT-013
---



## Changes

### Task 1: Extract `github_issues_types()` to a method on `Config`

Audit finding 4. Two identical `github_issues_types(config) -> Vec<&str>` functions exist in `src/cli/setup.rs:53-61` and `src/cli/init.rs:113-121`. A boolean variant `has_github_issues_types()` exists in `src/cli/validate.rs:18-24`.

Add two methods to `DocumentConfig` in `src/engine/config.rs`:

- `config.documents.github_issues_types() -> Vec<&str>` returns type names where `store == GithubIssues`
- `config.documents.has_github_issues_types() -> bool` convenience wrapper

Delete the three local copies. Update all call sites in `setup.rs`, `init.rs`, and `validate.rs`. Existing tests in `init.rs` (lines 142-157) and `validate.rs` (lines 169-185) should move to `config.rs` or be rewritten against the new methods.

AC: `github_issues_types` grep returns only `config.rs` and call sites. All existing tests pass.

### Task 2: Consolidate `resolve_repo()` into `src/engine/github.rs`

Audit finding 3. Four copies of `resolve_repo` exist:

- `src/engine/github.rs:8-15` (public, `Result<String>`)
- `src/cli/setup.rs:63-73` (`Result<String>`, identical logic)
- `src/cli/init.rs:68-75` (`Option<String>`, same logic different return)
- `src/cli/fetch.rs:90` (`Result<String>`, identical logic)

The `github.rs` version is already public and canonical. Delete the three local copies. Update callers to use `crate::engine::github::resolve_repo()`. For `init.rs` which needs `Option`, use `.ok()` at the call site.

Move any unique tests from `init.rs` (lines 161-175) to `github.rs` if not already covered.

AC: `fn resolve_repo` grep returns only `github.rs` definition. All existing tests pass.

### Task 3: Extract `extract_doc_id` to a shared location

Audit finding 2. Two functions with the same title-scanning logic:

- `src/engine/issue_cache.rs:356` `extract_doc_id(issue, type_name)`
- `src/tui/infra/event_loop.rs:30` `extract_doc_id_from_title(title, type_name)`

The `issue_cache.rs` version takes a `GhIssue` and extracts from the title. The `event_loop.rs` version operates on a raw title string.

Extract the title-based logic into a public function in `src/engine/issue_body.rs`: `pub fn extract_doc_id_from_title(title: &str, type_name: &str) -> Option<String>`.

Update both `issue_cache.rs` and `event_loop.rs` to call the shared version. Move tests from `event_loop.rs` (lines 449-477) to `issue_body.rs`.

AC: `fn extract_doc_id` grep shows one definition in `issue_body.rs` plus thin wrappers. Tests pass.

### Task 4: Consolidate `make_type` test helpers

Audit finding 5. Three test modules define near-identical `make_type(name, store) -> TypeDef`:

- `src/cli/setup.rs:82`
- `src/cli/validate.rs:114`
- `src/cli/init.rs:128`

Add `TypeDef::test_fixture(name: &str, store: StoreBackend) -> TypeDef` behind `#[cfg(test)]` in `src/engine/config.rs`. Delete the three local `make_type` functions and update call sites.

AC: `fn make_type` grep returns zero results. Tests pass.

### Task 5: Consolidate mock Gh clients into shared test module

Audit finding 17. Three separate mock implementations:

- `src/engine/gh.rs:489` `MockGhClient` (canned data)
- `src/engine/store_dispatch.rs:426` `MockGhClient` (records calls, most capable)
- `src/cli/setup.rs:158` `SetupMockGh` (reader + auth only)

Create a shared `MockGhClient` in a `#[cfg(test)]` submodule of `src/engine/gh.rs` with builder methods for per-test customization. Use the `store_dispatch.rs` version as the starting point since it records calls and has `with_*` builders.

Export so other test modules can import. Update `store_dispatch.rs` and `setup.rs` tests. The `validate.rs` mock only needs `GhAuth` and can use the shared mock with defaults.

AC: `struct SetupMockGh` grep returns zero results. Single `MockGhClient` definition in `gh.rs`. All tests pass.

## Test Plan

- `cargo test` passes after each task
- `grep` confirms each duplication target reduced to single definition
- No public API behavior changes

## Notes

Tasks ordered to minimize conflicts: config methods first (1-2), shared extraction (3), test infrastructure (4-5). Each task is independently shippable.
