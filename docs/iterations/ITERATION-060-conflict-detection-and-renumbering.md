---
title: Conflict detection and renumbering
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: docs/stories/STORY-059-conflict-detection-and-renumbering.md
---



## Test Plan

- Two flat-file docs with same ID (e.g. `RFC-020-foo.md`, `RFC-020-bar.md`): older date keeps number, newer renumbered (AC 1)
- Same ID, identical dates: earlier mtime wins (AC 2)
- Renumbered doc: file on disk renamed from old prefix to new (e.g. `RFC-020-bar.md` -> `RFC-021-bar.md`) (AC 3)
- Doc whose title contains old ID: title updated in frontmatter (AC 4)
- Subfolder conflict (e.g. `RFC-020-bar/index.md`): entire directory renamed, index.md and children move (AC 5)
- Three docs sharing same ID: oldest wins, two losers get distinct next-available numbers (AC 6)
- `--dry-run`: no files renamed or modified, output reports planned changes (AC 7)
- `--json` output: top-level shape is `{ field_fixes, conflict_fixes }`, not a flat array (AC 8)
- Each `ConflictFixResult` has `old_path`, `new_path`, `old_id`, `new_id`, `written` (AC 9)
- No conflicts: `conflict_fixes` is `[]`, existing field-fix results unchanged (AC 10)

All tests go in `tests/cli_fix_test.rs` using the existing `TestFixture` harness. Mtime tests use `filetime` crate to set mtime explicitly.

## Changes

### 1. Add `FixOutput` and `ConflictFixResult` structs

ACs: 8, 9, 10

**Modify:** `src/cli/fix.rs`

Rename existing `FixResult` to `FieldFixResult`. Add new structs:

```
FixOutput { field_fixes: Vec<FieldFixResult>, conflict_fixes: Vec<ConflictFixResult> }
ConflictFixResult { old_path, new_path, old_id, new_id, written }
```

Update `run`, `run_json`, `run_human` to return/format `FixOutput` instead of `Vec<FixResult>`. When there are no conflicts, `conflict_fixes` is empty and field-fix logic is unchanged.

**Verify:** existing tests still pass (`cargo test --test cli_fix_test`). Add test `fix_json_output_shape` asserting top-level keys are `field_fixes` and `conflict_fixes`.

### 2. Build ID-frequency map and detect conflicts

ACs: 1, 6

**Modify:** `src/cli/fix.rs`

In `collect_results` (or a new `detect_conflicts` fn), iterate `store.all_docs()`, group by `doc.id`. Any ID with >1 doc is a conflict group. Expose `extract_id_from_name` as `pub` in `src/engine/store.rs` if needed.

**Verify:** unit test creating 2 docs with same ID prefix, asserting detection returns the conflict group.

### 3. Implement oldest-wins priority with mtime tiebreak

ACs: 1, 2

**Modify:** `src/cli/fix.rs`

For each conflict group, sort docs by `date` ascending. On tie, read `fs::metadata(path).modified()` and sort by mtime. First doc is the winner (keeps number), rest are losers.

**Verify:** test with two docs same ID, different dates -> older wins. Test with same date, different mtimes -> earlier mtime wins (use `filetime` crate in test). Add `filetime` as a dev-dependency in `Cargo.toml`.

### 4. Renumber losers: flat-file rename + frontmatter title update

ACs: 3, 4

**Modify:** `src/cli/fix.rs`

For each loser, call `next_number` from `src/engine/template.rs` (make it `pub` if not already) to get the next available number for that type's directory. Build the new filename by replacing the old ID prefix with the new one. `fs::rename` the file. If frontmatter `title` contains the old ID prefix string, use `rewrite_frontmatter` from `src/engine/document.rs` to replace it.

**Verify:** test flat-file rename happened on disk. Test title containing old ID is updated. Test title without old ID is unchanged.

### 5. Renumber losers: subfolder rename

AC: 5

**Modify:** `src/cli/fix.rs`

When the conflicting doc's path ends in `index.md`, the rename target is the parent directory. `fs::rename` the directory (index.md and children move automatically). The new directory name has the new ID prefix replacing the old one.

**Verify:** test creating a subfolder doc with `write_subfolder_doc`, triggering a conflict, asserting the directory was renamed and `index.md` + child files exist at the new path.

### 6. Wire `--dry-run` for conflict resolution

AC: 7

**Modify:** `src/cli/fix.rs`

When `dry_run` is true, skip `fs::rename` and `rewrite_frontmatter` calls but still populate `ConflictFixResult` with `written: false`. Human output prints "Would rename X -> Y".

**Verify:** test `--dry-run` leaves files untouched, output contains planned renames.

### 7. Update main.rs dispatch (if needed) and human output formatting

ACs: 8, 10

**Modify:** `src/cli/fix.rs` (human format fn), `src/main.rs` (only if signature changes)

Add conflict fix results to human-readable output. Print "Renamed X -> Y" or "Would rename X -> Y" lines after existing field-fix lines.

**Verify:** end-to-end test with both field fixes and conflict fixes present in output.

## Notes

The `references_updated` field from the RFC is deliberately empty (`Vec::new()`) in this iteration -- reference cascade is Story 2's concern. The `ReferenceUpdate` struct is not needed yet.
