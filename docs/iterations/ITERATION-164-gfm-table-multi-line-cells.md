---
title: GFM table multi-line cells
type: iteration
status: accepted
author: agent
date: 2026-05-07
tags: []
related:
- implements: STORY-117
---



## Changes

1. **Fix `TableExtractor` break handling** [AC1, AC5] — DONE (defensive split applied; see Findings)
   - File: `src/tui/content/gfm/parse.rs:76`
   - Applied: `Event::SoftBreak => push(' ')`, `Event::HardBreak => push('\n')` (defensive — pulldown-cmark does not emit HardBreak inside table cells under current behaviour, but parallels `AdmonitionExtractor` and guards future emission).
   - Verify: `cargo test gfm --lib` (passing).

1b. **Handle `Event::InlineHtml("<br>")` in `TableExtractor`** [AC1, AC5]
   - File: `src/tui/content/gfm/parse.rs` (`TableExtractor::feed`, around line 76)
   - Add arm: `Event::InlineHtml(t) if t.eq_ignore_ascii_case("<br>") || t.eq_ignore_ascii_case("<br/>") || t.eq_ignore_ascii_case("<br />") => self.cell_text.push('\n')`
   - Why: GFM tables emit `<br>` as `Event::InlineHtml` (verified empirically in Task 1). Trailing-two-space form ends the row before pulldown-cmark sees a hard break, so `<br>` is the only path for in-cell line breaks. Currently dropped silently → cell becomes `"firstsecond"`.
   - TDD: add failing test `table_extractor_br_tag_emits_newline` first: input `"| col |\n|---|\n| first<br>second |"` → asserted `Table.rows[0][0]` contains `'\n'` between `first` and `second`. Variant test `table_extractor_br_tag_case_insensitive` for `<BR>`, `<br/>`, `<br />`.
   - Verify: `cargo test gfm --lib`

2. **Multi-line cell render in `render_table`** [AC1-AC6]
   - File: `src/tui/content/gfm/render.rs:37`
   - Per cell: split on `'\n'`, then `textwrap::wrap(segment, col_width)` per segment, concat → `Vec<Cow<str>>` cell visual lines.
   - `row_height = row.iter().map(|c| c_lines.len()).max()`.
   - Emit `row_height` `Line`s per row. For visual line `i` of row: span per cell = `align_text(cell_lines.get(i).map(AsRef::as_ref).unwrap_or(""), col_width, alignment)`, separator `" │ "` between cells. Short cells blank-pad via `align_text("", w, _)` (existing path).
   - Header row stays 1 visual line (ACs scope rows; headers assumed short).
   - Col-width calc: keep current `cell.len()` heuristic but treat newlines: use `cell.split('\n').map(str::len).max()` per cell so width reflects longest segment, not byte length including `\n`. Then existing `total > available → scale` branch unchanged.
   - Verify: `cargo test gfm --lib`, `cargo run` smoke.

3. **AC tests** [AC1-AC6]
   - File: `src/tui/content/gfm.rs` `#[cfg(test)] mod tests`
   - `table_cell_hard_break_splits_lines` (AC1): build `GfmTable { headers: ["h"], alignments: [None], rows: [["line1\nline2"]] }`. `render_table(&t, 80)` returns 4 `Line`s (header, sep, row line 1, row line 2). Assert line count and that line 2 starts with `"line1"`, line 3 starts with `"line2"`.
   - `table_cell_soft_wrap_word_boundary` (AC2): cell `"the quick brown fox"`, narrow `max_width` forcing col_width ≈ 10. Assert no rendered row line text wider than its col_width.
   - `table_row_height_max_lines` (AC3): row `["a\nb\nc", "x"]` → 3 row visual lines. Cell B blank on lines 2 + 3 (assert spans contain whitespace pad).
   - `table_single_line_unchanged` (AC4): table no `\n`, all cells short → row count = headers + sep + len(rows). Regression guard.
   - `table_combined_hard_soft` (AC5): cell `"short\nthis is a long segment that wraps"` with col_width forcing 2-line wrap on second segment. Assert 3 visual lines for that cell (`short`, wrap1, wrap2).
   - `table_alignment_preserved_multiline` (AC6): build 2-col multi-line row. Assert byte offset of `'│'` in each rendered row line is identical across the row's visual lines (use `Line::to_string()` then `find('│')`).
   - `table_extractor_br_tag_emits_newline` (AC1 parser path): markdown `"| a |\n|---|\n| first<br>second |"` → `extract_gfm_segments` first `Table.rows[0][0]` contains `'\n'` between `first` and `second`. (Trailing-two-space form ends the row in pulldown-cmark — see Findings — so `<br>` is the test fixture.)
   - `table_extractor_soft_break_renders_as_space` (AC2 parser path): plain cell w/o `<br>` → no `'\n'`, single space between segments. (Already added in Task 1.)
   - Per dictum Testing: behavioral (assert rendered content), isolated (per-test fixtures), deterministic (literal strings), structure-insensitive (use `render_table` / `extract_gfm_segments` public APIs).

4. **Manual TUI smoke** [AC7]
   - `cargo run`, open doc with multi-line table cell. Suggested fixture: temp md with `| col1 | col2 |\n|---|---|\n| line1<br>line2 | a long phrase that should wrap |`. (Use trailing-two-space form: `line1  \nline2` since GFM tables don't accept literal newlines in source — see Notes.)
   - Confirm preview pane (`panels.rs:39` path) renders multi-line.
   - Press `Enter` → fullscreen reader (`panels.rs:1104` path). Confirm same.
   - Note in iteration close: visual check passed for both panes.

5. **Validate** [all ACs]
   - `cargo test --all`
   - `cargo clippy --all-targets -- -D warnings`
   - `lazyspec validate --json`

## Findings

- Task 1: pulldown-cmark **never emits `Event::HardBreak` (or `SoftBreak`) inside GFM table cells** under current behaviour. Verified by enumerating events for three input forms:
  1. `| first<br>second |` → `Text("first") InlineHtml("<br>") Text("second")` — no break event.
  2. `| first  \nsecond |` (trailing-two-space) → splits into two `TableRow`s; the `\n` ends the row before inline parsing sees a hard break.
  3. `| first\\\nsecond |` (backslash-newline) → also splits into two rows.
- Consequence: defensive HardBreak → `\n` arm at `parse.rs:76` is correct + parallel to `AdmonitionExtractor`, but on its own does not produce in-cell `\n`. Task 1b added to handle `Event::InlineHtml("<br>")` (the only working in-cell line-break path under current pulldown-cmark).

## Test Plan

Per dictum Testing: behavioral, isolated, deterministic, structure-insensitive, predictive, real types.

- AC1 — `table_cell_hard_break_splits_lines` (render) + `table_extractor_preserves_hard_break` (parse). Two tests: one isolates render given a known `GfmTable`, one isolates parser given source markdown.
- AC2 — `table_cell_soft_wrap_word_boundary`: narrow col forces wrap; assert each visual line ≤ col width.
- AC3 — `table_row_height_max_lines`: differing cell line counts; assert row height = max.
- AC4 — `table_single_line_unchanged`: plain table → 1 line per row. Regression guard for AC1-3 changes.
- AC5 — `table_combined_hard_soft`: hard split first then soft wrap; line count = sum across segments.
- AC6 — `table_alignment_preserved_multiline`: separator `│` byte offset invariant across row's visual lines.
- AC7 — covered by single render path. `render_gfm_segments` → `render_table` shared by preview pane (`panels.rs:39`) and fullscreen reader (`panels.rs:1163` via `render_markdown_segment`). No separate code path; tests on `render_table` cover both. Manual smoke (task 4) validates.

Out of test scope:
- Widget snapshot diff of `Paragraph` render output (asserts ratatui internals; per dictum "predictive").
- Pixel-width assertions (terminal-dependent).

Tradeoff: AC2 + AC6 tests inspect rendered `Line` content (semi-structural). Justified — col width and alignment ARE the behavior under test. Mitigated by going through public `render_table` API.

## Notes

- `textwrap = "0.16"` already in `Cargo.toml` (ITER-162). No new deps. Default `Options` (word boundary) suffices per RFC-040 §2.
- `TableExtractor` SoftBreak/HardBreak collapse same bug class as `AdmonitionExtractor` fixed in ITER-163. Per dictum 5 (Rust idioms): match CommonMark spec.
- `FootnoteExtractor` (`parse.rs:212`) collapses both into `\n` — out of scope (footnote rendering not in STORY-117 ACs). Flag for separate iteration if footnote multi-line becomes target.
- `align_text` uses `text.len()` bytes. Pre-existing latent issue with non-ASCII / wide chars — out of scope unless ACs hit it.
- GFM tables in pulldown-cmark accept hard break inside cells via trailing-two-space `  \n` or backslash `\\\n`. Source markdown cannot contain literal newline mid-cell (table parser splits rows on `\n`). Test fixtures must use trailing-two-space.
- AC7 satisfied without code change beyond `render_table` because preview pane and fullscreen reader share `render_gfm_segments`. Confirmed by single grep in resolve-context: `panels.rs:39` (preview) and `panels.rs:1163` (fullscreen via `render_markdown_segment`) are the only callers.
- Header row stays single line (out of ACs).
- Per principle 6: no new abstraction. Modify existing `render_table` in place; one site changes.
