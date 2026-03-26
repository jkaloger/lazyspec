---
title: Fix subfolder document display name in TUI
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: STORY-031
---




## Changes

### Task 1: Extract a display_name helper and fix all three call sites

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Three places in `ui.rs` use `path.file_stem()` to derive a display name. For folder-based documents (`index.md`), this returns `"index"` instead of the folder name.

Add a helper function `display_name(path: &Path) -> &str` that returns:
- The parent directory name when `file_stem()` is `"index"`
- The `file_stem()` otherwise (existing behavior)

Replace all three `file_stem()` call sites (lines ~136, ~367, ~577) with the helper.

**How to verify:**
`cargo test` + manual TUI check with a folder-based doc

---

### Task 2: Add a test for the display_name helper

**Files:**
- Modify: `src/tui/ui.rs` (unit test module)

**What to implement:**

Add a `#[cfg(test)]` module with tests for `display_name`:
1. Flat file path returns the file stem (e.g. `docs/rfcs/RFC-001-foo.md` -> `RFC-001-foo`)
2. Subfolder index path returns the folder name (e.g. `docs/rfcs/RFC-002-bar/index.md` -> `RFC-002-bar`)

**How to verify:**
`cargo test`

## Test Plan

| Test | What it verifies |
|------|-----------------|
| `display_name_flat_file` | Flat `.md` paths return file stem |
| `display_name_subfolder_index` | Subfolder `index.md` paths return folder name |

## Notes

Bug introduced by ITERATION-020 subfolder discovery. The TUI was using `file_stem()` which returns `"index"` for folder-based documents. The same pattern from `resolve_shorthand()` (check parent dir when filename is `index.md`) applies here.
