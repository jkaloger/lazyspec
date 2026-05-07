---
title: GFM table multi-line cells
type: story
status: accepted
author: jkaloger
date: 2026-05-07
tags: []
related:
- implements: RFC-040
---



## Context

Implements RFC-040 section 2: GFM table cells in TUI markdown preview must render as multi-line when content contains hard newlines or exceeds the column width. Currently table cells render as a single line, truncating or collapsing multi-line content. This makes spec tables and any GFM table with wrapped prose unreadable in preview and fullscreen reader.

This Story extends RFC-040 to include soft-wrap inside cells (originally listed as deferred). Soft-wrap activates when cell text exceeds the column's allotted width; word-boundary wrapping produces additional visual lines without changing column widths.

## Acceptance Criteria

- **AC1: Hard newline split**
  **Given** a GFM table cell containing one or more `\n` characters
  **When** the table renders in TUI preview
  **Then** the cell displays N visual lines, one per `\n`-delimited segment

- **AC2: Soft wrap at word boundary**
  **Given** a GFM table cell whose text width exceeds its column's allotted width
  **When** the table renders
  **Then** the cell text wraps at word boundaries onto multiple visual lines, none exceeding column width

- **AC3: Row height equals max post-wrap line count**
  **Given** a row whose cells have differing post-wrap line counts
  **When** the row renders
  **Then** the row's height equals the maximum line count across its cells, and shorter cells leave their extra lines blank

- **AC4: Short single-line cells unchanged**
  **Given** a row where every cell fits in a single line with no `\n`
  **When** the row renders
  **Then** the row renders as a single visual line, identical to current behaviour

- **AC5: Combined hard newlines and soft wrap**
  **Given** a cell containing both `\n` and a segment longer than column width
  **When** the cell renders
  **Then** hard newlines split first, then each segment soft-wraps at word boundaries; the cell's line count equals the sum of wrapped lines per segment

- **AC6: Column alignment preserved**
  **Given** a multi-line row
  **When** the row renders
  **Then** column boundaries align across all visual lines of the row, matching single-line row alignment

- **AC7: Preview pane and fullscreen reader**
  **Given** a markdown document containing a multi-line table
  **When** viewed in the TUI preview pane and in the fullscreen reader
  **Then** both render the table with multi-line cells per AC1-AC6

## Scope

### In Scope

- Table render path in `src/tui/content/gfm/render.rs` (or equivalent)
- Hard `\n` splitting per cell
- Soft wrap at word boundary using a Rust ecosystem crate (e.g. `textwrap`) per principle 5
- Row-height calculation as max wrapped-line count across cells
- Coverage in preview pane and fullscreen reader render paths

### Out of Scope

- Cell-level markdown styling changes (existing span renderer assumed sufficient)
- Configurable wrap policy (word vs character) — fixed to word boundary
- Caching wrapped output across renders
- Changes to non-table markdown rendering (covered by STORY-116)
- Document list table rows (covered by STORY-115)
