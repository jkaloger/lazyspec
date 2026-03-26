---
title: Reference cascade
type: iteration
status: draft
author: agent
date: 2026-03-13
tags: []
related:
- implements: STORY-060
---



## Test Plan

| Test | AC | What it verifies |
|------|-----|-----------------|
| `cascade_rewrites_related_entry` | AC-1 | Doc A's `related` entry pointing at B's old path is rewritten to B's new path |
| `cascade_rewrites_body_ref` | AC-2 | `@ref` directive in body pointing at old path is rewritten to new path |
| `cascade_subfolder_rewrites_child_paths` | AC-3 | Renaming `RFC-020-foo/` to `RFC-021-foo/` updates all references to child docs under old dir |
| `cascade_dry_run_no_writes` | AC-4 | With `--dry-run`, no files modified on disk; JSON still contains `ReferenceUpdate` entries |
| `cascade_json_contains_reference_updates` | AC-5 | `references_updated` populated with correct `file`, `field`, `old_value`, `new_value` |
| `cascade_no_refs_empty_array` | AC-6 | When no docs reference the old path, `references_updated` is `[]` and no files change |

## Changes

### Task 1: Add `ReferenceUpdate` and `ConflictFixResult` structs

**ACs addressed:** AC-5

**Files:**
- Modify: `src/cli/fix.rs`

Add the two new structs alongside `FixResult`. `ConflictFixResult` holds `old_path`, `new_path`, `old_id`, `new_id`, `references_updated: Vec<ReferenceUpdate>`, and `written: bool`. `ReferenceUpdate` holds `file: String`, `field: String` (either `"related"` or `"body"`), `old_value: String`, `new_value: String`. Both derive `Debug, Serialize, Clone`.

Add a top-level `FixOutput` struct that wraps `field_fixes: Vec<FixResult>` and `conflict_fixes: Vec<ConflictFixResult>`. Update `run`, `run_json`, and `run_human` to return `FixOutput` instead of `Vec<FixResult>`. The existing field-fix logic populates `field_fixes`; `conflict_fixes` stays empty until Task 3.

**Verify:** `cargo check` compiles. `cargo test cli_fix` still passes (output shape changes but test assertions on the inner array remain valid after adjusting test code).

---

### Task 2: Implement `cascade_references` function

**ACs addressed:** AC-1, AC-2, AC-3, AC-6

**Files:**
- Modify: `src/cli/fix.rs`

Add a function:
```rust
fn cascade_references(
    root: &Path,
    store: &Store,
    old_path: &str,
    new_path: &str,
    dry_run: bool,
) -> Vec<ReferenceUpdate>
```

The function iterates every document in `store.all_docs()` and for each:

1. **Related entries** -- read the file, parse frontmatter YAML as `serde_yaml::Value`, walk the `related` sequence. For each mapping value that contains `old_path` as a substring, record a `ReferenceUpdate { field: "related", .. }` and replace the substring with `new_path` in the YAML value. If any replacements occurred and `!dry_run`, rewrite the frontmatter using the pattern from `rewrite_frontmatter`.

2. **Body @ref directives** -- use `regex::Regex` with `refs::REF_PATTERN` to find `@ref` matches in the body. For each match whose path component contains `old_path` as a substring, record a `ReferenceUpdate { field: "body", .. }` and replace the old path substring with `new_path` in the body text. If any replacements occurred and `!dry_run`, write the full file back (frontmatter + updated body).

For subfolder documents (AC-3), the caller invokes `cascade_references` once for the parent path and once for each child path that changed. The path replacement is a simple string substitution (e.g. `RFC-020-foo/` to `RFC-021-foo/`), which naturally covers children since their paths share the directory prefix.

Return all collected `ReferenceUpdate` entries. If no matches found, return an empty vec (AC-6).

**Verify:** Unit test via `cascade_no_refs_empty_array` -- call with paths that no document references, assert empty vec returned.

---

### Task 3: Wire cascade into conflict resolution flow

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5

**Files:**
- Modify: `src/cli/fix.rs`

This task assumes Story 1 (STORY-059) has been implemented, providing the conflict detection and renumbering loop. After Story 1 renames a file/directory and produces `old_path`/`new_path`, call `cascade_references(root, store, &old_path, &new_path, dry_run)` and attach the returned vec to `ConflictFixResult.references_updated`.

For subfolder renumbers, the directory rename changes the path prefix for all children. Call `cascade_references` with the directory prefix (e.g. `docs/rfcs/RFC-020-foo/` to `docs/rfcs/RFC-021-foo/`). This single call covers both `index.md` and child paths since the replacement is prefix-based.

Set `ConflictFixResult.written` based on `dry_run` (AC-4): if `dry_run`, set to `false` even though updates were computed.

**Verify:** `cargo test` for `cascade_dry_run_no_writes` and `cascade_json_contains_reference_updates`.

---

### Task 4: Integration tests

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Create: `tests/cli_fix_cascade_test.rs`

Write the six tests from the test plan table above. Each test uses `common::TestFixture` to set up a temp directory with documents that have cross-references, then calls the cascade function (or `fix::run_json` once Story 1 is integrated) and asserts on both the returned `ReferenceUpdate` entries and the on-disk file contents.

Key setup patterns:
- AC-1: Doc A with `related: [{implements: docs/rfcs/RFC-020-foo.md}]`, doc B at `RFC-020-foo.md`. Call cascade with old=`docs/rfcs/RFC-020-foo.md`, new=`docs/rfcs/RFC-021-foo.md`. Assert A's frontmatter now references the new path.
- AC-2: Doc A body contains `@ref docs/rfcs/RFC-020-foo.md#SomeStruct`. After cascade, body contains `@ref docs/rfcs/RFC-021-foo.md#SomeStruct`.
- AC-3: Subfolder `RFC-020-foo/` with children. Cascade with directory prefix. Assert child refs updated.
- AC-4: Same as AC-1 but `dry_run: true`. Assert file unchanged on disk, but `ReferenceUpdate` vec is non-empty.
- AC-5: Parse JSON output, assert `references_updated` array has entries with all four fields.
- AC-6: No docs reference old path. Assert empty `references_updated` and no file writes.

**Verify:** `cargo test cli_fix_cascade`

## Notes

This iteration depends on Story 1 (STORY-059) for the conflict detection and renumbering logic. Tasks 1-2 and 4 can be implemented independently. Task 3 wires them together once Story 1 lands. The `cascade_references` function is intentionally decoupled from conflict detection so it can be tested in isolation.
