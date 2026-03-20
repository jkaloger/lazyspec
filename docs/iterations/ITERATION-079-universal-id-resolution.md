---
title: Universal ID Resolution
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-068-universal-id-resolution.md
---



## Changes

### Task 1: Extract shared resolution helper

**ACs addressed:** AC-1 (shared resolution helper)

**Files:**
- Create: `src/cli/resolve.rs`
- Modify: `src/cli/mod.rs` (add `pub mod resolve;`)
- Modify: `src/cli/show.rs` (remove private `resolve_shorthand_or_path`, import from `resolve`)

**What to implement:**

Move the private `resolve_shorthand_or_path` function from `src/cli/show.rs:21-26` into a new `src/cli/resolve.rs` module as a public function. The signature stays the same:

```rust
pub fn resolve_shorthand_or_path<'a>(
    store: &'a Store,
    id: &str,
) -> Result<&'a DocMeta, ResolveError>
```

Update `show.rs` to import and use the shared version. Add the module declaration to `src/cli/mod.rs`.

Also add a convenience function that returns the resolved path (since most mutation commands need the path, not the DocMeta):

```rust
pub fn resolve_to_path(store: &Store, id: &str) -> Result<PathBuf, ResolveError>
```

This calls `resolve_shorthand_or_path` and returns `doc.path.clone()`.

**How to verify:**
- `cargo test` passes (show tests still work)
- `cargo run -- show RFC-028` still works

### Task 2: Wire resolution into `link` and `unlink`

**ACs addressed:** AC-2 (link shorthand), AC-3 (link backwards compat), AC-4 (unlink shorthand)

**Files:**
- Modify: `src/cli/link.rs` (change signatures to accept `&Store`, use `resolve_to_path`)
- Modify: `src/main.rs` (load Store before calling link/unlink)

**What to implement:**

Change `link` and `unlink` function signatures to accept a `&Store` parameter in addition to `root`:

```rust
pub fn link(root: &Path, store: &Store, from: &str, rel_type: &str, to: &str) -> Result<()>
pub fn unlink(root: &Path, store: &Store, from: &str, rel_type: &str, to: &str) -> Result<()>
```

Inside each function, resolve `from` and `to` using `resolve_to_path`. If resolution fails (not found or ambiguous), map the `ResolveError` to an `anyhow` error and return early.

In `main.rs`, load a `Store` for the `Link` and `Unlink` arms before calling the handlers. Print the resolved paths in the success message (not the raw user input), so the user sees what was actually linked.

Backwards compatibility: `resolve_shorthand_or_path` tries exact path first, so full paths continue to work unchanged.

**How to verify:**
- `cargo run -- link RFC-028 related-to STORY-068` creates the link with canonical paths in frontmatter
- `cargo run -- link docs/rfcs/RFC-028-document-reference-ergonomics.md related-to docs/stories/STORY-068-universal-id-resolution.md` still works
- `cargo run -- unlink STORY-068 related-to RFC-028` removes the link

### Task 3: Wire resolution into `delete`, `update`, `ignore`, `unignore`

**ACs addressed:** AC-5 (delete/update/ignore/unignore shorthand), AC-7 (backwards compat)

**Files:**
- Modify: `src/cli/delete.rs` (add `store` param, use `resolve_to_path`)
- Modify: `src/cli/update.rs` (add `store` param, use `resolve_to_path`)
- Modify: `src/cli/ignore.rs` (add `store` param to both functions, use `resolve_to_path`)
- Modify: `src/main.rs` (load Store for these command arms)

**What to implement:**

Same pattern as Task 2. Add `&Store` parameter to each handler. Resolve the path argument using `resolve_to_path` before performing the operation. Update `main.rs` dispatch to load a Store for `Update`, `Delete`, `Ignore`, `Unignore` arms.

For `update` and `delete`, print the resolved path in the success message. For `ignore`/`unignore`, same.

**How to verify:**
- `cargo run -- update STORY-068 --status accepted` updates the correct file
- `cargo run -- delete` with a shorthand ID deletes the correct file
- `cargo run -- ignore STORY-068` / `cargo run -- unignore STORY-068` work
- Full paths still work for all four commands

### Task 4: Error handling and tests

**ACs addressed:** AC-6 (ambiguous ID error), AC-7 (not-found error), all ACs (regression)

**Files:**
- Modify: `tests/cli_link_test.rs` (add shorthand tests, ambiguous/not-found tests)
- Modify: `tests/cli_mutate_test.rs` (add shorthand tests for update/delete)
- Modify: `tests/cli_ignore_test.rs` (add shorthand tests for ignore/unignore)

**What to implement:**

Add tests for each command using shorthand IDs. The test fixture already provides `write_rfc`, `write_story`, etc. For tests that need a Store, call `fixture.store()`.

Tests to add:

1. `link_with_shorthand_ids` - link using shorthand, verify canonical paths in frontmatter
2. `link_with_full_paths_still_works` - existing behaviour preserved
3. `unlink_with_shorthand_ids` - unlink using shorthand
4. `link_ambiguous_id_returns_error` - create two docs with similar IDs, verify error
5. `link_not_found_id_returns_error` - use nonexistent ID, verify error
6. `update_with_shorthand_id` - update status using shorthand
7. `delete_with_shorthand_id` - delete using shorthand
8. `ignore_with_shorthand_id` - ignore using shorthand
9. `unignore_with_shorthand_id` - unignore using shorthand

Each test creates a fixture, loads a store, calls the command with a shorthand ID, and asserts the expected outcome.

Note: the existing `link` and `unlink` function signatures will change (new `store` param), so existing tests in `cli_link_test.rs` need updating to pass a store. Same for `cli_mutate_test.rs` and `cli_ignore_test.rs`.

**How to verify:**
- `cargo test` passes all new and existing tests

## Test Plan

| Test | AC | Properties traded |
|------|-----|-------------------|
| `link_with_shorthand_ids` | AC-2 | Isolated, Behavioral, Specific |
| `link_with_full_paths_still_works` | AC-3, AC-7 | Behavioral, Structure-insensitive |
| `unlink_with_shorthand_ids` | AC-4 | Isolated, Behavioral |
| `link_ambiguous_id_returns_error` | AC-6 | Specific, Deterministic |
| `link_not_found_id_returns_error` | AC-7 | Specific, Deterministic |
| `update_with_shorthand_id` | AC-5 | Isolated, Behavioral |
| `delete_with_shorthand_id` | AC-5 | Isolated, Behavioral |
| `ignore_with_shorthand_id` | AC-5 | Isolated, Behavioral |
| `unignore_with_shorthand_id` | AC-5 | Isolated, Behavioral |

All tests use `TestFixture` for isolation. No integration tests needed since the resolution logic is already well-tested in `store_test.rs`; these tests verify the wiring.

## Notes

The main architectural change is that `link`, `unlink`, `delete`, `update`, `ignore`, and `unignore` will now require a loaded `Store`. This adds a small cost (Store load time) to these commands, but Store loading is already fast enough for interactive use (used by `show`, `context`, `list`, `search`, `validate`, `fix`, and the TUI).

The `context` command currently uses `store.resolve_shorthand` directly without the path fallback. This is a pre-existing inconsistency but is out of scope for this iteration (no AC covers it).
