---
title: Fix gh CLI flag compatibility (AUDIT-010)
type: iteration
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: STORY-097
---



## Changes

### Task 1: Remove `--json` from `issue_create` and use `issue_view` follow-up

ACs addressed: STORY-097 AC "Init creates labels" (indirectly, via working issue creation)

Files:
- Modify: `src/engine/gh.rs` (GhCli::issue_create)

What to implement:

Remove the `--json` line from `issue_create` args (line 261). `gh issue create` outputs a URL like `https://github.com/owner/repo/issues/42\n` to stdout on success. Extract the issue number from that URL (parse the last path segment as u64), then call `self.issue_view(repo, number)` to get the full `GhIssue` struct with JSON fields. This preserves the existing `Result<GhIssue>` return type and keeps callers (e.g. `store_dispatch.rs:157`) unchanged.

Add a helper `fn parse_issue_number_from_url(url: &str) -> Result<u64>` that extracts the trailing number from a GitHub issue URL.

How to verify:
- `cargo test -p lazyspec` passes
- Task 3 tests cover this

### Task 2: Tighten `classify_error` auth detection patterns

ACs addressed: STORY-097 AC "Validate warns on missing gh or auth" (correct error classification)

Files:
- Modify: `src/engine/gh.rs` (classify_error function)

What to implement:

Replace the broad `lower.contains("login")` check at line 84 with more specific patterns that won't match gh usage text. Use patterns like `"not logged in"`, `"gh auth login"`, `"authentication required"`, or `"authentication token"`. Keep the existing `"auth"` and `"401"` checks but remove bare `"login"`.

The usage text from `gh issue create` contains "Assign people by their login" which currently false-matches. After this change, only genuine auth errors should classify as `GhError::AuthFailure`.

How to verify:
- `cargo test -p lazyspec` passes
- Task 3 tests cover this

### Task 3: Add unit tests for the fixes

ACs addressed: all three findings

Files:
- Modify: `src/engine/gh.rs` (tests module)

What to implement:

1. Test `parse_issue_number_from_url` with valid URL (`https://github.com/owner/repo/issues/42` -> 42), URL with trailing newline, and invalid URL (returns error).

2. Test `classify_error` does NOT return `AuthFailure` when stderr contains gh usage text with "login" in a non-auth context. Use the actual stderr from `gh issue create --json`:
   ```
   unknown flag: --json\n\nUsage:  gh issue create [flags]\n\nFlags:\n  -a, --assignee login   Assign people by their login.
   ```
   Assert it returns `GhError::ApiError`, not `GhError::AuthFailure`.

3. Test `classify_error` still returns `AuthFailure` for genuine auth errors like `"You are not logged in to any GitHub hosts. Run gh auth login"`.

How to verify:
- `cargo test -p lazyspec -- gh` passes

## Test Plan

- `parse_issue_number_from_url` with valid URL extracts number correctly (isolated, fast, specific)
- `parse_issue_number_from_url` with trailing whitespace still works (deterministic)
- `parse_issue_number_from_url` with invalid URL returns error (specific)
- `classify_error` with gh usage text containing "login" does NOT produce AuthFailure (behavioral, predictive)
- `classify_error` with real auth error still produces AuthFailure (behavioral, regression guard)
- Existing `classify_error` tests continue to pass (structure-insensitive)

## Notes

Standalone bug fix iteration from AUDIT-010 findings 1-3. The three findings form a fix chain: removing `--json` (finding 1) requires a new parsing strategy for stdout (finding 3), and the error classifier (finding 2) is an independent fix that prevents the misleading error message.
