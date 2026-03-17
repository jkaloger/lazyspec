---
title: Sort documents by date
type: iteration
status: accepted
author: agent
date: 2026-03-17
tags: []
related:
- implements: docs/stories/STORY-064-sqids-numbering-and-config.md
---



## Context

All document lists sort by path (lexicographic). For incremental IDs (`RFC-001`, `RFC-002`) this approximates chronological order because the numeric prefix increases over time. For sqids IDs (`RFC-k3f`, `RFC-mQ7`) path sorting produces arbitrary order since encoded strings have no chronological relationship.

The `DocMeta.date` field (`chrono::NaiveDate`) is already present on every document but is only used as a sort key in `src/cli/fix.rs` during renumbering. Switching all document list sorts to use `date` as the primary key (with `path` as tiebreaker) gives consistent chronological ordering regardless of numbering strategy.

## Changes

### Task 1: Extract a shared sort comparator

**Files:**
- Modify: `src/engine/document.rs`

**What to implement:**

Add a comparison function on `DocMeta` (or a free function) that sorts by `date` descending (newest first), then by `path` ascending as tiebreaker:

```rust
pub fn sort_by_date_desc(a: &DocMeta, b: &DocMeta) -> std::cmp::Ordering {
    b.date.cmp(&a.date).then_with(|| a.path.cmp(&b.path))
}
```

Centralising this avoids repeating the sort logic in 8+ call sites and makes future sort changes a one-line fix.

**How to verify:**
- `cargo build` succeeds

---

### Task 2: Update TUI sorts to use date-based ordering

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Replace the path-based sort calls with the new comparator at these locations:

| Line | Function | Current |
|------|----------|---------|
| 481 | `build_doc_tree()` top-level | `a.path.cmp(&b.path)` |
| 519 | `build_doc_tree()` children | `a.path.cmp(&b.path)` |
| 560 | `filtered_docs()` | `a.path.cmp(&b.path)` |
| 756 | `docs_for_current_type()` | `a.path.cmp(&b.path)` |

Leave the relation tree sorts (lines 688, 726) unchanged -- those sort by type/title which is correct for that context.

**How to verify:**
- `cargo build` succeeds
- Launch TUI with a project containing sqids-numbered docs; verify they appear in date order

---

### Task 3: Update CLI and store sorts to use date-based ordering

**Files:**
- Modify: `src/cli/list.rs`
- Modify: `src/cli/status.rs`
- Modify: `src/engine/store.rs`

**What to implement:**

- `src/cli/list.rs`: After calling `store.list()`, sort results using the date comparator before output (currently unsorted).
- `src/cli/status.rs` line 36: Replace `a.path.cmp(&b.path)` with the date comparator.
- `src/engine/store.rs` line 481 (`search()`): Replace `a.doc.path.cmp(&b.doc.path)` with date-based sort on `a.doc` and `b.doc`.

**How to verify:**
- `cargo run -- list --json | jq '.[].date'` shows dates in descending order
- `cargo run -- status --json` shows documents ordered by date
- `cargo run -- search "some term" --json` shows results ordered by date

---

## Test Plan

### Test 1: Sort comparator ordering (unit test)

Create a unit test in `src/engine/document.rs` that constructs several `DocMeta` values with different dates and paths, sorts them with the comparator, and asserts:

- Documents with newer dates appear before older dates
- Documents with the same date are ordered by path ascending
- Single-element and empty vectors are handled

Properties: Isolated, Fast, Deterministic, Specific, Behavioral.

### Test 2: CLI list output is date-ordered (integration test)

In a test project with documents created on different dates, run `cargo run -- list --json` and verify the output is ordered by date descending.

Properties: Predictive, Behavioral. Trades Fast for confidence in the full CLI path.

## Notes

- The TUI search results sort at line 995 (`results.sort()`) sorts path strings, not `DocMeta`. This may need a separate fix if search results should also be date-ordered, but it depends on whether search returns full `DocMeta` at that call site. Leaving it out of scope for now.
- `src/cli/fix.rs` already sorts by date for renumbering (line 717). No change needed there.
