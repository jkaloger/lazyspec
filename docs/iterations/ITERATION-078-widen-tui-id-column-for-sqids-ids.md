---
title: Widen TUI ID column for sqids IDs
type: iteration
status: accepted
author: agent
date: 2026-03-18
tags: []
related:
- implements: STORY-064
---




## Context

ITERATION-077 changed sqids IDs from ~3 chars to ~6 chars (timestamp-based input). The longest display ID is now `ITERATION-<6chars>` = 17 chars. The TUI ID column is hardcoded at 14 chars, which will truncate sqids IDs on longer prefixes.

## Changes

### Task 1: Widen ID column and format string

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

In `doc_table_widths()` (line 250), change the ID column constraint from `Constraint::Length(14)` to `Constraint::Length(18)`. This accommodates `ITERATION-` (10 chars) + 6-char sqids ID + 2 chars padding for the `! ` duplicate prefix.

In `doc_row_cells()` (line 272), update the format string from `format!("{:<14}", id)` to `format!("{:<18}", id)` to match.

**How to verify:**
```
cargo test
```
Then visually confirm with `cargo run` that IDs render without truncation.

## Test Plan

- All existing TUI tests pass (column width change doesn't break layout logic)
- Visual: `ITERATION-077` and longer sqids IDs render fully in the ID column

## Notes

The agent session ID column also uses `Constraint::Length(14)` but agent session IDs are a fixed format unrelated to document IDs, so it should not be changed.
