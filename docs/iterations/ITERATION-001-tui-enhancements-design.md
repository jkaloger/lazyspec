---
title: "TUI Enhancements Design"
type: iteration
status: rejected
author: "jkaloger"
date: 2026-03-05
tags: [tui, design]
related:
  - implements: docs/stories/STORY-003-tui-dashboard.md
---


## Overview

Three TUI enhancements to improve the dashboard's visual clarity and navigation capabilities.

## Feature 1: Colour-coded tags

Tags rendered as individually coloured spans using a deterministic hash-to-palette mapping. A fixed palette of 8-10 terminal-safe colours ensures the same tag always maps to the same colour across the TUI.

### Preview panel

The current preview header is a single markdown string piped through `tui_markdown::from_str`. Replace this with a manually constructed `Vec<Line>` of styled spans. Tags appear as coloured inline text like `[implementation]` in their assigned colour.

The markdown body continues to use `tui_markdown::from_str` and is appended below the styled header.

### Document list

Tags appear after the status column as small coloured text. Space permitting, show the first 2-3 tags; truncate with `+N` if there are more.

### Colour assignment

```rust
fn tag_color(tag: &str) -> Color {
    const PALETTE: &[Color] = &[
        Color::Magenta, Color::Cyan, Color::Green,
        Color::Yellow, Color::Blue, Color::Red,
        Color::LightMagenta, Color::LightCyan,
        Color::LightGreen, Color::LightBlue,
    ];
    let hash = tag.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[(hash as usize) % PALETTE.len()]
}
```

## Feature 2: Double-border active panel indicator

Active panel uses `BorderType::Double` (╔═╗), inactive panels use `BorderType::Plain` (┌─┐). The existing cyan vs dark grey colour distinction remains as a secondary visual cue.

### Changes

In `draw_type_panel` and `draw_doc_list`, set `.border_type()` on the `Block` based on whether the panel is active. Requires importing `ratatui::widgets::BorderType`.

## Feature 3: Tabbed preview with Relations tab

The preview panel gains two tabs: **Preview** (existing behaviour) and **Relations** (new).

### App state additions

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewTab {
    Preview,
    Relations,
}
```

New fields in `App`:
- `preview_tab: PreviewTab` (default: `Preview`)
- `selected_relation: usize` (selection index within relations list)

### Tab switching

`Tab` key cycles between Preview and Relations tabs. Only available when `active_panel == DocList` or when a document is selected.

### Tab indicator

The preview panel border title shows: ` Preview | Relations ` with the active tab in cyan bold and the inactive tab in dark grey.

### Relations tab rendering

Relations for the selected document are fetched via `Store::related_to()`. They are displayed as a navigable list grouped by relation type:

```
  implements
    > STORY-001 Document Model       accepted
      STORY-002 CLI Commands         accepted

  blocks
      ITER-002 Phase Two             draft
```

Each relation item shows:
- Relation type as a section header (styled, e.g. italic or dimmed)
- Target document title (resolved via `Store::get()`)
- Target document status (colour-coded)
- `>` indicator for the currently selected item

### Navigation within Relations tab

- `j/k` moves selection within the relations list
- `Enter` navigates to the selected related document (same logic as `select_search_result`: find the doc type, switch panel selection)
- `Tab` switches back to Preview tab

### RelationType display

Add `Display` impl for `RelationType`:
- `Implements` -> "implements"
- `Supersedes` -> "supersedes"
- `Blocks` -> "blocks"
- `RelatedTo` -> "related to"

## Files modified

| File | Changes |
|------|---------|
| `src/tui/app.rs` | Add `PreviewTab` enum, `preview_tab` and `selected_relation` fields, navigation methods for relations, `navigate_to_relation` method |
| `src/tui/ui.rs` | `tag_color` function, styled preview header, tags in doc list, double borders, tabbed preview rendering, relations tab rendering |
| `src/engine/document.rs` | `Display` impl for `RelationType` |
| `src/tui/mod.rs` | Handle `Tab` key event, relation navigation keys |

## Test plan

- Verify tag colours are consistent (same tag always produces same colour)
- Verify active panel uses double border, inactive uses single
- Verify Tab key switches between Preview and Relations tabs
- Verify relations list shows all forward and reverse relations for selected document
- Verify Enter on a relation navigates to that document
- Verify documents with no relations show an empty state message
