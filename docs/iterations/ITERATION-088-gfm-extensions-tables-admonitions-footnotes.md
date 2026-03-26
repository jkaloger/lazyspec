---
title: 'GFM extensions: tables, admonitions, footnotes'
type: iteration
status: accepted
author: agent
date: 2026-03-18
tags: []
related:
- implements: STORY-067
---



## Context

`tui-markdown` 0.3.7 hardcodes its pulldown-cmark parser options and does not enable `ENABLE_TABLES`, `ENABLE_GFM`, or `ENABLE_FOOTNOTES`. Even if enabled, the rendering code has `warn!()` stubs for tables, footnotes, and ignores `BlockQuoteKind` for admonitions.

Strikethrough (AC-4) and task lists (AC-5) already work since `tui-markdown` enables those flags and implements rendering. This iteration covers the remaining three ACs.

**Approach:** Add `pulldown-cmark` as a direct dependency. Pre-process the markdown body with GFM flags enabled. Extract tables, admonitions, and footnotes into new `PreviewSegment` variants. Render these with custom ratatui logic. Pass remaining markdown through `tui-markdown` as before.

## Changes

### Task 1: Add pulldown-cmark dependency and GFM segment extraction

**ACs addressed:** AC-1 (tables), AC-2 (admonitions), AC-3 (footnotes)

**Files:**
- Modify: `Cargo.toml`
- Create: `src/tui/gfm.rs`
- Modify: `src/tui/mod.rs`

**What to implement:**

Add `pulldown-cmark` as a direct dependency in `Cargo.toml` (pin to the same version `tui-markdown` uses, visible in `Cargo.lock`).

Create `src/tui/gfm.rs` with a function `extract_gfm_segments(body: &str) -> Vec<GfmSegment>` that parses the body with pulldown-cmark using `ENABLE_TABLES | ENABLE_GFM | ENABLE_FOOTNOTES | ENABLE_STRIKETHROUGH | ENABLE_TASKLISTS`. Walk the event stream and split the body into segments:

- `GfmSegment::Markdown(String)` -- plain markdown, passed to `tui-markdown` as before
- `GfmSegment::Table(GfmTable)` -- struct holding `headers: Vec<String>`, `alignments: Vec<Alignment>`, `rows: Vec<Vec<String>>`
- `GfmSegment::Admonition { kind: String, body: String }` -- blockquotes with a `BlockQuoteKind` (Note, Warning, Tip, Important, Caution)
- `GfmSegment::Footnote { label: String, body: String }` -- footnote definitions, collected and appended at the end

The function reassembles non-GFM events back into markdown strings for the `Markdown` variant, so `tui-markdown` handles everything it already supports.

Register the module in `src/tui/mod.rs`.

**How to verify:**
```
cargo test --lib tui::gfm
```

### Task 2: Render GFM segments as ratatui Lines

**ACs addressed:** AC-1 (tables), AC-2 (admonitions), AC-3 (footnotes)

**Files:**
- Modify: `src/tui/gfm.rs`

**What to implement:**

Add rendering functions that convert each `GfmSegment` variant to `Vec<Line>`:

`render_table(table: &GfmTable, max_width: u16) -> Vec<Line>`:
- Calculate column widths based on content, constrained by `max_width`
- Render header row with bold styling
- Render a separator row using `─` characters
- Render data rows with `│` column separators
- Respect alignment (left/center/right) from the table's alignment vec

`render_admonition(kind: &str, body: &str) -> Vec<Line>`:
- Render a label line with the admonition type (e.g. "NOTE", "WARNING") in a distinct color per kind
- Render the body as blockquote-style indented text with a colored left border prefix
- Color mapping: Note=blue, Tip=green, Important=magenta, Warning=yellow, Caution=red

`render_footnotes(footnotes: &[(String, String)]) -> Vec<Line>`:
- Render a separator line
- For each footnote, render `[^label]: definition` with the label styled distinctly

Also add a top-level `render_gfm_segments(segments: &[GfmSegment], max_width: u16) -> Vec<Line>` that dispatches to the above and collects footnotes to append at the end.

**How to verify:**
```
cargo test --lib tui::gfm
```

### Task 3: Integrate GFM rendering into preview pipeline

**ACs addressed:** AC-1, AC-2, AC-3

**Files:**
- Modify: `src/tui/ui.rs` (functions: `draw_preview_content` ~line 447, fullscreen preview ~line 810)

**What to implement:**

In both `draw_preview_content` and the fullscreen preview rendering path, replace the direct `tui_markdown::from_str(text)` calls inside the `PreviewSegment::Markdown` match arm:

1. For each `PreviewSegment::Markdown(text)`, call `gfm::extract_gfm_segments(text)`
2. For `GfmSegment::Markdown` sub-segments, continue passing through `tui_markdown::from_str` as today
3. For `GfmSegment::Table`, `GfmSegment::Admonition`, `GfmSegment::Footnote`, call the corresponding `gfm::render_*` function and append the resulting lines
4. Pass `area.width` (or `content_width`) to `render_table` for column width calculation

Extract a helper function (e.g. `render_markdown_segment(text: &str, max_width: u16) -> Vec<Line>`) to avoid duplicating this logic between the two preview paths.

**How to verify:**
```
cargo test
cargo run -- show docs/rfcs/RFC-017-better-markdown-preview.md
```
Visually confirm tables, admonitions, and footnotes render correctly. The RFC itself contains no tables, so also test with a document that has a markdown table.

## Test Plan

**Unit tests in `src/tui/gfm.rs`:**

1. `test_extract_plain_markdown` -- input with no GFM features returns a single `Markdown` segment. Verifies the extraction doesn't break normal content. (Isolated, fast, structure-insensitive)

2. `test_extract_table` -- input containing a pipe table extracts a `Table` segment with correct headers, alignments, and rows. (Behavioral, specific)

3. `test_extract_admonition` -- input containing `> [!NOTE]\n> body` extracts an `Admonition` segment with kind="Note" and the body text. Test each variant (Note, Warning, Tip, Important, Caution). (Behavioral, specific)

4. `test_extract_footnotes` -- input with `[^1]: definition` extracts a `Footnote` segment. (Behavioral, specific)

5. `test_extract_mixed` -- input with a table, admonition, and regular markdown returns segments in the correct order with correct types. (Composable, predictive)

6. `test_render_table_alignment` -- a table with left/center/right aligned columns renders with correct alignment characters and column separators. (Behavioral, readable)

7. `test_render_admonition_kinds` -- each admonition kind produces a label line with the correct type string. (Specific, fast)

8. `test_render_footnotes` -- footnotes render with `[^label]:` prefix and definition text. (Behavioral, specific)

> [!NOTE]
> These are unit tests on the extraction and rendering functions in isolation. Integration with the actual TUI preview is verified manually via `cargo run` since ratatui widget rendering is difficult to test in a headless environment without snapshot infrastructure.

## Notes

- `pulldown-cmark` version should match what `tui-markdown` 0.3.7 uses transitively (check `Cargo.lock`) to avoid duplicate versions in the dependency tree.
- The `GfmSegment` extraction needs to handle the case where a table or admonition appears mid-paragraph -- pulldown-cmark events for these are block-level, so they naturally split at paragraph boundaries.
- Footnote references inline (`[^1]`) can be rendered as styled text by the extraction step, replacing them with e.g. `[1]` in superscript style before passing to `tui-markdown`.
