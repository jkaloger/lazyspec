---
title: AUDIT-013 type system polish
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- related-to: AUDIT-013
---



## Changes

### Task 1: Remove hardcoded type list from `extract_type_and_tags`

Audit finding 13. `extract_type_and_tags` at `src/engine/issue_body.rs:115-140` hardcodes five known types (`RFC, STORY, ITERATION, ADR, SPEC`). Custom types configured in `.lazyspec.toml` with `store = "github-issues"` are silently dropped, and the document defaults to `spec`.

Change the function signature to accept a list of known type names (from config):

```rust
fn extract_type_and_tags(labels: &[String], known_types: &[&str]) -> (DocType, Vec<String>)
```

Update all callers to pass the configured type names. The `deserialize` function (line 57) needs to accept the type list as well, or accept a context struct that includes it. The existing `IssueContext` is a natural place to add a `known_types: Vec<String>` field.

Write a test with a custom type (e.g. `lazyspec:task`) that verifies it is recognized when passed in the known types list, and defaults to `spec` when not.

AC: No hardcoded type list in `issue_body.rs`. Custom types are recognized via config. Tests pass.

### Task 2: Rename `is_non_lifecycle_status` to `needs_frontmatter_status`

Audit finding 14. `is_non_lifecycle_status` at `src/engine/issue_body.rs:90-92` returns true for statuses that _cannot_ be reconstructed from open/closed alone. The name is misleading because `review`, `accepted`, and `in-progress` are lifecycle statuses that still return `true` (they need frontmatter storage).

Rename to `needs_frontmatter_status`. The logic is correct; only the name is wrong. Update the single call site at line 29.

AC: `is_non_lifecycle_status` grep returns zero results. `needs_frontmatter_status` exists. Tests pass.

### Task 3: Expand `GhError` enum with structured error variants

Audit finding 12. `GhError` at `src/engine/gh.rs:35-38` has only `NotInstalled`. Auth failures, rate limits, and API errors are reported as untyped `anyhow` strings, making programmatic error handling impossible.

Add variants:

- `AuthFailed(String)` for `gh auth status` failures
- `ApiError { status: u16, message: String }` for non-2xx API responses
- `RateLimited { retry_after: Option<u64> }` for 429 responses

Update `GhCli` methods (`run_gh_checked`, `auth_status`, etc.) to return structured errors where the gh CLI output provides enough signal. Callers that currently string-match on error messages can pattern-match instead.

AC: `GhError` has at least 3 variants. `GhCli::run_gh_checked` returns `GhError::ApiError` for non-zero exits where parseable. Tests pass.

### Task 4: Add `Display` impl for `StoreBackend`

Audit finding 18. Error messages and debug output format `StoreBackend` via `Debug` or hardcode strings. A `Display` impl gives consistent user-facing formatting.

Add `impl Display for StoreBackend` in `src/engine/config.rs` matching the serde rename: `Filesystem` -> `"filesystem"`, `GithubIssues` -> `"github-issues"`.

Replace any hardcoded `"github-issues"` or `"filesystem"` strings used for display purposes with the `Display` impl.

AC: `impl Display for StoreBackend` exists. Tests pass.

## Test Plan

- `cargo test` passes after each task
- Task 1: new test with custom type recognized via config-driven type list
- Task 3: new tests for `GhError::AuthFailed` and `GhError::ApiError` construction from gh CLI output
- Task 4: unit test for `StoreBackend::display()` output

## Notes

This iteration is independent of the 130->131->132 chain. Tasks are ordered by impact: task 1 (correctness bug) first, cosmetic changes (tasks 2, 4) last.
