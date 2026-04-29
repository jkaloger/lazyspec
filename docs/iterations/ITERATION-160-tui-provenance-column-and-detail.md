---
title: TUI provenance column and detail
type: iteration
status: accepted
author: agent
date: 2026-04-29
tags: []
related:
- implements: STORY-112
---


## Changes

Story AC #1 says "alongside Author column" but no Author column in doc list table. Reinterpret: append `Provenance` column after `tags` in list view; render `Provenance:` line in preview header below `Tags:`. No new Author column. Read-only — no forms wiring.

1. **Widen column constraint set.** `src/tui/views/panels.rs:215` `doc_table_widths`. Bump return `[Constraint; 6]` → `[Constraint; 7]`. Tags `Constraint::Length(24)` (capped). Append `Constraint::Min(20)` for provenance (flex slot). ACs: 1.

2. **Append provenance cell in `doc_row_cells`.** `src/tui/views/panels.rs:256`. Add `provenance: &[String]` param after `tags`. Compute `joined = provenance.join(", ")`. Build `provenance_cell` from `Span::styled(truncate_with_ellipsis(&joined, 20), style)`. Empty list → empty `Span::raw("")`. Return cell vec extended with `provenance_cell`. ACs: 1, 2, 3, 4.

3. **Truncation helper.** New private fn in `src/tui/views/panels.rs`:
   ```rust
   fn truncate_with_ellipsis(s: &str, max_cols: usize) -> String
   ```
   Walk chars accumulating display width via `unicode_width::UnicodeWidthChar`. If full string fits, return clone. Else truncate to fit `max_cols - 1` cols, push `'…'`. Edge: `max_cols == 0` → `""`. `max_cols == 1` → `"…"` if input non-empty. Verify `unicode-width` available via `cargo tree -p unicode-width`; if absent, fall back to `chars().count()`.

4. **Thread provenance into `doc_row_for_node`.** `src/tui/views/panels.rs:327`. Near tags lookup line 363 add `let provenance = app.store.get(&node.path).map(|d| d.provenance.clone()).unwrap_or_default();`. Pass slice into `doc_row_cells` after tags arg.

5. **Preview header provenance line.** `src/tui/views/panels.rs:568` (after the `if !doc.tags.is_empty()` block). Mirror tags pattern:
   ```rust
   if !doc.provenance.is_empty() {
       let mut spans = vec![Span::raw(" Provenance: ")];
       for (idx, entry) in doc.provenance.iter().enumerate() {
           if idx > 0 { spans.push(Span::raw(", ")); }
           spans.push(Span::raw(entry));
       }
       lines.push(Line::from(spans));
   }
   ```
   Skip line entirely when empty (AC6 — section absent). ACs: 5, 6.

6. **Refactor preview header into testable fn.** Extract preview header line construction (lines 544-590 of `panels.rs`) into:
   ```rust
   pub(super) fn build_preview_header_lines(
       doc: &DocMeta,
       expanding: bool,
   ) -> Vec<Line<'static>>
   ```
   Returns lines vec including title, type/status/author, date, tags, provenance, expansion notice. `render_document_preview` calls it then extends with body segments. Behavioural unit-test entrypoint per DICTUM-004 "Writable / Readable".

7. **Update test helpers.** `src/tui/views/panels.rs:1282,1294`. Add `provenance: &[String]` arg to `doc_row_cells_for_test` and `doc_row_cells_gh_for_test`. Pass through to `doc_row_cells`.

8. **Verify call chain.** `doc_row_cells` → only `doc_row_for_node` → only `draw_doc_list`. Single chain. `cargo check` clean.

9. **No `forms.rs` change.** AC7 satisfied by absence. No `FormField::Provenance` variant. No overlay entry.

10. **No `update.rs` / mutation paths touched.** Read-only story.

## Test Plan

Unit tests in `src/tui/views/panels.rs` `mod tests`.

### Truncation helper

- `truncate_no_change_when_fits` — `("ab", 5)` → `"ab"`.
- `truncate_appends_ellipsis_when_overflows` — `("abcdef", 4)` → `"abc…"`. AC3.
- `truncate_zero_width_returns_empty` — `("abc", 0)` → `""`.
- `truncate_one_width_returns_ellipsis` — `("abc", 1)` → `"…"`.

### Row cells

- `doc_row_cells_appends_provenance_cell` — non-empty provenance → returned vec length grew by 1 vs prior; last cell text equals joined string. AC1.
- `doc_row_cells_provenance_empty_when_list_empty` — empty slice → last cell renders empty. AC4.
- `doc_row_cells_provenance_comma_joined` — `["A","B","C"]` → last cell text `"A, B, C"`. AC2.
- `doc_row_cells_provenance_truncated_overflow` — input joined width > 20 → last cell text ends with `'…'`. AC3.

### Preview header

- `preview_header_includes_provenance_when_present` — `DocMeta` with `provenance: ["X","Y"]` → `build_preview_header_lines` output contains a line with text `"Provenance:"` and both entries (joined into spans). AC5.
- `preview_header_omits_provenance_when_empty` — empty list → no line contains `"Provenance:"`. AC6.

### Read-only verification

AC7 not asserted via test. Verified by absence of `FormField::Provenance` and absence of overlay edit affordance. Documented in Notes; relies on code review + CI for drift.

### Tradeoffs

- **Truncation width as constant `20`** vs runtime-resolved: `Constraint::Min(20)` is the floor; on wide terminals real width exceeds 20, so ellipsis appears prematurely. Cost: minor visual; matches `tags` cell behaviour today. Computing resolved widths means threading `Layout::split` results into row builder — disproportionate. Defer.
- **Preview header refactor**: small change; enables behavioural unit-test rather than Frame snapshot. Aligned with DICTUM-004.
- **Unicode-width**: free-form citations may contain non-ASCII. Prefer `unicode-width::UnicodeWidthChar` if already in dep tree; else `char::len_utf8 == 1 ? 1 : 1` simple fallback (`chars().count()`) — non-CJK acceptable.

## Notes

- Engine field already in `DocMeta.provenance: Vec<String>` (ITERATION-158). All literals updated. CLI shipped (ITERATION-159).
- Read-only: no `forms.rs`, no `update.rs`, no overlay changes.
- Fullscreen header (`render_fullscreen_document` line 813) out of story scope — story specifies "list views" and "detail panel"; fullscreen is separate surface. Skip unless asked.
- AC7 untested by automated test. Absence-based satisfaction.
- Manual smoke after build: `cargo run` → fixture doc with provenance entries → list shows column populated, detail panel header shows entries; doc without → empty cell, no detail line.
