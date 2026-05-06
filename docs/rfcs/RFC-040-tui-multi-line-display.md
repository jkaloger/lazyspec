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

Enable multi-line display in the TUI through expandable rows with a keybinding toggle:

- **Document list tables**: Rows default to 1 line; pressing `e` toggles expansion to show the full content (up to a max height). A visual indicator (▸ for collapsed, ▾ for expanded) shows which rows can be expanded.

- **Markdown preview**: Multi-line table cells and content with embedded newlines render correctly. The existing GFM table renderer (from RFC-017/STORY-067) needs to preserve newline characters within cells and render them as line breaks.

## Design

### 1. Document List Tables — Expandable Rows

**Toggle mechanism:**
- Default: 1 line per row (current behavior)
- Press `e` with row selected: expand to fit content (max height from config, default 5 lines)
- Press `e` again: collapse back to 1 line
- Visual indicator in first column: `▸` = expandable/collapsed, `▾` = expanded

**Configuration:**
```rust
@draft MultiLineConfig {
    max_expanded_height: usize,  // default: 5
    indicator_collapsed: String,  // default: "▸"
    indicator_expanded: String,   // default: "▾"
}
```

**State tracking:**
```rust
@draft ExpandedRows {
    expanded: HashSet<usize>,  // indices of expanded rows
}
```

**Rendering changes:**
- Modify `doc_list_node_spans` (or equivalent table rendering) to accept row height
- When expanded, split content at newline boundaries or wrap text to multiple lines
- Truncate with "…" if content exceeds `max_expanded_height`

**Keybinding:**
- `e` in document list view: toggle expansion for selected row
- No effect if row content fits in 1 line (no indicator shown)

### 2. Markdown Preview — Multi-Line Cell Rendering

**GFM table cells:**
- Current implementation likely collapses whitespace/newlines within cells
- Fix: preserve `\n` within table cell content and render as line breaks
- Reuse `tui_markdown`'s paragraph rendering for cell content

**Implementation approach:**
- Modify the table rendering in `src/tui/content/gfm/render.rs`
- After parsing table cells, check for embedded newlines
- Render multi-line cells as multiple `Line` items within the table row
- Adjust row height calculation to accommodate multi-line cells

**Markdown content with newlines:**
- Ensure `render_gfm_segments` preserves intentional line breaks (not just paragraph breaks)
- Soft wraps (lines exceeding terminal width) handled by `Paragraph::wrap`
- Hard newlines (explicit `\n` in source) render as actual line breaks

### 3. Interface Sketches

**Expanded row state:**
```rust
@draft App {
    // Existing fields...
    expanded_rows: ExpandedRows,
    multiline_config: MultiLineConfig,
}
```

**Table row rendering (pseudocode):**
```
for (i, row) in rows.iter().enumerate() {
    let height = if app.expanded_rows.is_expanded(i) {
        min(content_lines(row), config.max_expanded_height)
    } else {
        1
    };
    let indicator = if content_lines(row) > 1 {
        if app.expanded_rows.is_expanded(i) { "▾" } else { "▸" }
    } else { " " };
    // render row at height...
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

1. **Expandable document list rows** — Add `e` keybinding to toggle row expansion in document lists (search, filtered views). Show visual indicators for expandable rows. Configure max height.

2. **Markdown table multi-line cells** — Fix GFM table rendering to preserve and display newlines within table cells. Ensure tables with multi-line content render correctly in preview panel and fullscreen mode.

3. **Markdown newline preservation** — Ensure explicit newlines in markdown content render as line breaks (not collapsed) in preview. Verify with admonitions, code blocks, and mixed content.

## Configuration

Add to `~/.config/lazyspec/config.toml`:

```toml
[tui.multiline]
max_expanded_height = 5
indicator_collapsed = "▸"
indicator_expanded = "▾"
```

## Out of Scope

- Auto-wrapping (always expand to full height vs. fixed max) — can be added later as config option
- Half-page navigation within expanded rows — standard scroll behavior applies
- Persisting expansion state across TUI sessions — expansion is session-local
