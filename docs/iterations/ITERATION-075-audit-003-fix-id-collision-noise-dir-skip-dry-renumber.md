---
title: 'AUDIT-003 fix: ID collision, noise dir skip, DRY renumber'
type: iteration
status: accepted
author: agent
date: 2026-03-17
tags: []
related:
- implements: docs/stories/STORY-066-fix-numbering-format-conversion.md
- related-to: docs/audits/AUDIT-006-rfc-027-sqids-implementation-quality-review.md
---





## Changes

### Task 1: Fix ID collision in sqids-to-incremental conversion

**Findings addressed:** AUDIT-003 Finding 3 (high)

**Files:**
- Modify: `src/cli/fix.rs` (lines 250-277, `collect_renumber_fixes` `Incremental` branch)
- Test: `tests/cli_fix_renumber_test.rs`

**What to implement:**

The `RenumberFormat::Incremental` branch currently assigns numbers starting at 1 regardless of existing incremental docs. When a directory contains both `RFC-001-foo.md` (incremental) and `RFC-abc-bar.md` (sqids), converting to incremental assigns `RFC-001` to the sqids doc, colliding with the existing file.

Fix: before assigning new numbers, collect the set of existing incremental IDs for this type. Start numbering above the maximum existing ID. Specifically:

1. Collect all docs for the type that already have incremental IDs (filter where `is_incremental_id` is true)
2. Parse their numeric segments and find the max
3. Assign new numbers starting from `max + 1` instead of `1`

**How to verify:**
```
cargo test --test cli_fix_renumber_test renumber_sqids_to_incremental_avoids_collision
```

### Task 2: Skip noise directories in scan_dir_for_references

**Findings addressed:** AUDIT-003 Finding 1 (medium)

**Files:**
- Modify: `src/cli/fix.rs` (lines 359-416, `scan_dir_for_references`)
- Test: `tests/cli_fix_renumber_test.rs`

**What to implement:**

Add a hardcoded skip-list for common noise directories at the top of `scan_dir_for_references`. When iterating directory entries, check if the directory name matches any entry in the skip-list and `continue` if so. This check should happen before the existing `managed_dirs` check.

Skip-list: `.git`, `target`, `node_modules`, `.venv`, `dist`, `build`, `.hg`.

The check should compare the directory's file name (not the full path), since these directories can appear at any depth.

**How to verify:**
```
cargo test --test cli_fix_renumber_test renumber_external_refs_skips_noise_dirs
```

### Task 3: Extract shared renumber logic (DRY)

**Findings addressed:** AUDIT-003 Finding 2 (low)

**Files:**
- Modify: `src/cli/fix.rs` (lines 98-157 `run_renumber`, lines 508-534 `run_renumber_json`)

**What to implement:**

Extract the shared logic from `run_renumber` and `run_renumber_json` into a helper function:

```rust
fn collect_renumber_output(
    root: &Path,
    store: &Store,
    config: &Config,
    format: &RenumberFormat,
    doc_type: Option<&str>,
    dry_run: bool,
) -> RenumberOutput
```

This function builds the `RenumberOutput` (format string, collect changes, scan external refs). Then:
- `run_renumber` calls it and handles human/json printing
- `run_renumber_json` calls it and serializes to string

No new tests needed -- existing `renumber_json_output_structure` already exercises both paths. Run the full test suite to confirm no regressions.

**How to verify:**
```
cargo test --test cli_fix_renumber_test
```

## Test Plan

- `renumber_sqids_to_incremental_avoids_collision`: Set up a directory with `RFC-001-foo.md` (incremental) and `RFC-abc-bar.md` (sqids). Run renumber to incremental. Assert `RFC-001-foo.md` still exists unchanged, and the sqids doc is renamed to `RFC-002-bar.md` (not `RFC-001`). Isolated, deterministic, specific.
- `renumber_external_refs_skips_noise_dirs`: Set up a fixture with a `.git/config` file and a `node_modules/pkg/README.md` that both contain a reference to an old filename. Run `scan_external_references`. Assert neither file appears in the results. Fast, isolated, behavioral.
- Existing tests run green with no changes: `cargo test --test cli_fix_renumber_test`

## Notes

Task ordering matters: Task 1 (collision fix) is the highest-priority data-loss bug. Task 3 (DRY) should come last since it restructures the functions that Tasks 1 and 2 may touch.
