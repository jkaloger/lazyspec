---
title: Fix numbering format conversion
type: iteration
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- implements: STORY-066
---




## Changes

### Task 1: Add `--renumber` and `--type` CLI flags to fix command

**ACs addressed:** AC-1, AC-2, AC-3, AC-5

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

**What to implement:**
Add two new options to the `Fix` variant in `Commands`: `--renumber <FORMAT>` (accepts `sqids` or `incremental`) and `--type <TYPE>` (optional, filters to a single document type like `rfc`). Pass both values through to `fix::run()`. When `--renumber` is provided, skip the existing field-fix and conflict-fix logic and instead run the new renumber logic (Task 2).

**How to verify:**
`cargo run -- help fix` shows the new flags. `cargo run -- fix --renumber sqids --dry-run --json` runs without error.

### Task 2: Implement renumber collection logic in fix.rs

**ACs addressed:** AC-1, AC-2, AC-3, AC-5, AC-7

**Files:**
- Modify: `src/cli/fix.rs`

**What to implement:**
Add a `collect_renumber_fixes` function. It iterates all docs from the store (filtered by `--type` if given). For each document, extract its current ID using `extract_id_from_name`. Determine the current format: if the ID segment after the prefix dash is all digits, it's incremental; otherwise it's sqids. Skip documents already in the target format (AC-7).

For incremental-to-sqids: decode the numeric ID, encode through `sqids::Sqids` using the project's `SqidsConfig` from config. For sqids-to-incremental: sort target documents alphabetically by filename, assign sequential zero-padded numbers starting from 1 (AC-2). Build a `ConflictFixResult`-like struct for each rename. When `dry_run` is false, call `std::fs::rename` and `update_title_in_file`. Add a new `RenumberFixResult` struct and extend `FixOutput` with a `renumber_fixes` field.

Wire the existing `cascade_references` function (already in fix.rs) to update all `related` frontmatter and `@ref` body directives after each rename (AC-4). Process all renames first to build the old-to-new path map, then cascade in a second pass to avoid partial updates.

**How to verify:**
`cargo test -- fix_renumber` passes. Manual: create a temp project with `RFC-001-foo.md` and `RFC-002-bar.md`, run `cargo run -- fix --renumber sqids --dry-run --json`, verify output lists expected renames.

### Task 3: External reference detection and summary

**ACs addressed:** AC-6

**Files:**
- Modify: `src/cli/fix.rs`

**What to implement:**
After renaming, scan non-lazyspec markdown files (any `.md` file not managed by the store) and any file matching common patterns (`*.wiki`, `README*`) for occurrences of the old filenames. Collect these as `ExternalReference { file: String, old_name: String, line: usize }`. Include them in the JSON output under `external_references`. In human output, print a summary like: `Warning: 3 external references found that could not be auto-updated` followed by the file/line list.

**How to verify:**
Create a `README.md` referencing `RFC-001-foo.md`, run renumber with `--dry-run`, verify the external reference appears in output.

### Task 4: Human-readable and JSON output for renumber

**ACs addressed:** AC-5

**Files:**
- Modify: `src/cli/fix.rs`

**What to implement:**
Extend `format_human` and the JSON serialization to include renumber results. For dry-run, prefix each line with "Would rename". For actual runs, prefix with "Renamed". Include reference updates in the output. The `FixOutput` struct gains `renumber_fixes: Vec<RenumberFixResult>` and `external_references: Vec<ExternalReference>`.

**How to verify:**
`cargo run -- fix --renumber sqids --dry-run` prints human-readable rename plan. `--json` flag produces parseable JSON with all fields.

## Test Plan

| AC | Test | Type |
|----|------|------|
| AC-1 | Create project with `RFC-001-foo.md`, run `fix --renumber sqids`, assert file renamed to `RFC-<sqid>-foo.md` | Integration |
| AC-2 | Create project with sqids-named docs, run `fix --renumber incremental`, assert files renamed to `RFC-001-...`, `RFC-002-...` in alphabetical order | Integration |
| AC-3 | Create project with RFCs and stories, run `fix --renumber sqids --type rfc`, assert only RFCs renamed | Integration |
| AC-4 | Create two docs where one has `related: [{implements: <old-path>}]`, run renumber, assert related path updated | Integration |
| AC-5 | Run any renumber with `--dry-run`, assert no files changed on disk, output lists planned renames | Integration |
| AC-6 | Create a README referencing a doc path, run renumber, assert external reference warning in output | Integration |
| AC-7 | Create mixed incremental + sqids docs, run `fix --renumber sqids`, assert only incremental docs converted | Integration |

## Notes

This iteration depends on the sqids numbering infrastructure from ITERATION-072 (the `SqidsConfig` struct and sqids crate dependency). The `cascade_references` function already exists in `fix.rs` and handles both `related` frontmatter and `@ref` body directives, so AC-4 is largely covered by reusing it.

For sqids-to-incremental conversion, alphabetical filename ordering produces a deterministic sequence per the RFC design. The numbering starts at 1, not at `next_number`, since the goal is a clean cutover.
