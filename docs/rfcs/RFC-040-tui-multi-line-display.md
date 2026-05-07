---
title: "TUI Multi-Line Display"
type: rfc
status: draft
author: "jkaloger"
date: 2026-04-30
tags: ["tui", "display", "ux"]
---

## Problem

The TUI currently displays document list rows and markdown content as single-line items. This creates two problems:

1. **Document list tables** (e.g., search results, filtered views): Long titles, paths, or tag lists are truncated with no way to see the full content. Users must select a document and open the fullscreen reader to see what was cut off.

2. **Markdown preview**: Tables and content with embedded newlines render as single lines, making multi-line table cells (common in spec tables, GFM tables with wrapped text) unreadable.

## Intent

Enable multi-line display in the TUI through a global wrap mode toggle:

- **Document list tables**: `x` toggles a TUI-wide `wrap_mode`. When wrap mode is on, the currently selected/hovered row wraps its content (title, tags, provenance) to multiple lines up to a configurable max height. All other rows stay at 1 line. (`e` is reserved for opening the external editor.) No per-row indicator — wrap mode is a global UI affordance.

- **Markdown preview**: Multi-line table cells and content with embedded newlines render correctly. The existing GFM table renderer (from RFC-017/STORY-067) needs to preserve newline characters within cells and render them as line breaks.

## Design

### 1. Document List Tables — Wrap Mode

**Toggle mechanism:**
- Default: `wrap_mode = false`, all rows 1 line.
- Press `x`: flip `wrap_mode` for the whole TUI.
- When `wrap_mode == true`, the row at `selected_doc` wraps content to fit (up to `max_expanded_height` lines from config). Selection movement (`j`/`k`) shifts which row wraps.
- No visual indicator on individual rows.

**Configuration:**
```rust
@draft MultiLineConfig {
    max_expanded_height: usize,  // default: 5
}
```

**State tracking:**
```rust
@draft App {
    // Existing fields...
    wrap_mode: bool,
}
```

**Rendering changes:**
- `DocCellWidths::from_area_width` mirrors ratatui's `Layout` over the table constraints so wrap measurements match real cell rects (column_spacing accounted for).
- Cell content for the selected row when `wrap_mode` is on: title and provenance pre-wrapped via `textwrap`; tags greedy-packed into styled `[name]` spans across multiple `Line`s without splitting tags; row height = `min(content_lines, max_expanded_height)`.

**Keybinding:**
- `x` toggles `wrap_mode` (works regardless of selection).
- `e` retains its existing binding (open external editor).

### 2. Markdown Preview — Multi-Line Cell Rendering

**GFM table cells:**
- Current implementation likely collapses whitespace/newlines within cells
- Fix: preserve `\n` within table cell content and render as line breaks
- Soft-wrap cell content at word boundary when text exceeds the column's allotted width (use a Rust ecosystem crate per principle 5, e.g. `textwrap`)
- Reuse `tui_markdown`'s paragraph rendering for cell content

**Implementation approach:**
- Modify the table rendering in `src/tui/content/gfm/render.rs`
- For each cell: split on hard `\n`, then soft-wrap each segment to column width
- Render wrapped lines as multiple `Line` items within the table row
- Row height = max wrapped-line count across cells in the row

**Markdown content with newlines:**
- Ensure `render_gfm_segments` preserves intentional line breaks (not just paragraph breaks)
- Soft wraps (lines exceeding terminal width) handled by `Paragraph::wrap`
- Hard newlines (explicit `\n` in source) render as actual line breaks

### 3. Interface Sketches

**Wrap mode state:** `pub wrap_mode: bool` on `App`.

**Table row rendering (pseudocode):**
```
for (i, row) in rows.iter().enumerate() {
    let expanded = app.wrap_mode && i == app.selected_doc;
    let height = if expanded {
        min(content_lines(row), config.ui.multiline.max_expanded_height)
    } else {
        1
    };
    // render row at height with wrapped or single-line cells...
}
```

**Markdown table cell:**
```rust
@ref src/tui/content/gfm/render.rs#render_table_row

// For each cell:
let lines: Vec<Line> = if cell_contains_newlines(cell) {
    cell.split('\n').map(Line::from).collect()
} else {
    vec![Line::from(cell)]
};
```

## Stories

1. **Expandable document list rows** — Add `x` keybinding to toggle a TUI-wide wrap mode; when on, the currently selected row wraps title/tags/provenance to multiple lines (up to configurable max). Configure max height.

2. **Markdown table multi-line cells** — Fix GFM table rendering to preserve hard `\n` within table cells and soft-wrap cell text at word boundaries when it exceeds column width. Ensure tables with multi-line content render correctly in preview panel and fullscreen mode.

3. **Markdown newline preservation** — Ensure explicit newlines in markdown content render as line breaks (not collapsed) in preview. Verify with admonitions, code blocks, and mixed content.

## Configuration

Add to `~/.config/lazyspec/config.toml`:

```toml
[tui.multiline]
max_expanded_height = 5
```

## Out of Scope

- Auto-wrapping (always expand to full height vs. fixed max) — can be added later as config option
- Half-page navigation within expanded rows — standard scroll behavior applies
- Persisting wrap mode across TUI sessions — session-local
- Per-row wrap toggling — wrap mode is global; only the hovered row wraps
