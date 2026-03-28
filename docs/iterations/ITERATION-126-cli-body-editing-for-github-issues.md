---
title: CLI body editing for github-issues
type: iteration
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: STORY-100
---



## Changes

### Task 1: Add --body and --body-file flags to the CLI

ACs addressed: "Update body via --body flag", "Update body via --body-file flag"

Files:
- Modify: `src/cli.rs` (Commands::Update variant)
- Modify: `src/main.rs` (update handler)

What to implement:

Add two new optional fields to the `Update` variant in `Commands` enum in `src/cli.rs`:
- `--body <string>` -- inline body content
- `--body-file <file>` -- path to file, or `-` for stdin

In `src/main.rs` (lines 84-96), extend the update handler to read the body value. If `--body-file` is provided, read the file contents (or stdin if `-`). If both `--body` and `--body-file` are given, error with "cannot use both --body and --body-file". Pass the body as a `("body", value)` tuple in the updates vec.

How to verify:
- `cargo run -- help update` shows `--body` and `--body-file` flags
- `cargo build` succeeds

### Task 2: Route body updates through store dispatch

ACs addressed: "Update body via --body flag", "Body update respects optimistic lock", "Body update works with other flags", "Filesystem documents ignore body flags"

Files:
- Modify: `src/engine/store_dispatch.rs` (GithubIssuesStore::update, lines 179-256)
- Modify: `src/engine/store_dispatch.rs` (FilesystemStore::update, lines 80-88)
- Modify: `src/cli/update.rs` (filesystem path, lines 45-62)

What to implement:

In `GithubIssuesStore::update` (store_dispatch.rs:213-225), add a `"body"` arm to the match on update keys. When `"body"` is provided, replace the body variable (which is currently the deserialized body from the remote issue) with the new value. The re-serialization at line 227 (`issue_body::serialize(&meta, &body)`) already uses this variable, so the new body will be included in the `issue_edit` call. No changes needed to the optimistic lock check or status lifecycle logic -- they already run regardless of which fields are updated.

In `FilesystemStore::update` (or `cli/update.rs` filesystem path), if a `"body"` key is present in updates, return an error: `"--body and --body-file are not supported for filesystem documents; edit the file directly"`. Check for this before doing any other work.

How to verify:
- `cargo test -p lazyspec` passes
- Task 3 tests cover this

### Task 3: Add tests for body update paths

ACs addressed: all five ACs

Files:
- Modify: `src/engine/store_dispatch.rs` (tests module, after existing update tests ~line 804)

What to implement:

Add tests using the existing `MockGhClient` and test helpers (`tmp_root`, `make_issue_body`, etc.):

1. `github_issues_update_body_success` -- create a store with a seeded issue map entry, call `update` with `[("body", "new content")]`, verify `issue_edit` was called (check via mock). Verify the new body is serialized correctly by inspecting the mock's captured `last_edit_body`.

   Note: the mock's `issue_edit` currently captures `last_edit_title` and `last_edit_labels_remove` but not body. Add a `last_edit_body: RefCell<Option<String>>` field to `MockGhClient` and capture it in the `issue_edit` impl.

2. `github_issues_update_body_with_status` -- call `update` with `[("body", "new"), ("status", "complete")]`, verify both body is updated and issue is closed.

3. `github_issues_update_body_optimistic_lock_failure` -- set up mismatched timestamps in issue map vs mock view response, call update with body, verify it fails with the conflict error.

4. `filesystem_update_rejects_body` -- create a `FilesystemStore`, call `update` with a `("body", "content")` tuple, verify it returns an error about unsupported body editing.

How to verify:
- `cargo test -p lazyspec -- update` passes

## Test Plan

- github-issues body update succeeds and serializes correctly (behavioral, isolated, fast)
- github-issues body + status update applies both changes (behavioral, composable)
- github-issues body update fails on optimistic lock mismatch (behavioral, specific)
- filesystem store rejects body update with clear error (behavioral, specific)
- existing update tests continue to pass (structure-insensitive)

## Notes

The `GithubIssuesStore::update` already re-serializes the full issue body on every update (even for status-only changes). Adding body editing is a small extension: swap the deserialized body variable before re-serialization. The optimistic lock, status lifecycle, and issue map refresh logic all apply unchanged.
