---
title: Status bar foundation
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: docs/stories/STORY-063-status-bar-foundation.md
---



## Changes

### Task 1: Add footer row to outer layout and render status bar

**ACs addressed:** AC-1 (footer visible on all screens), AC-2 (panel name in left), AC-3 (help hint in right)

**Files:**
- Modify: `src/tui/ui.rs` (lines 97-140, the `draw()` function)

**What to implement:**

Change the outer layout from a 2-row split to a 3-row split:

```rust
let outer = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(1),  // header
        Constraint::Min(0),    // content
        Constraint::Length(1),  // status bar (new)
    ])
    .split(f.area());
```

Add a `draw_status_bar(f, app, outer[2])` call after the ViewMode match block.

Implement `draw_status_bar`:
- Split `outer[2]` into 3 horizontal sections using `Layout` with `Constraint::Percentage(33)` for each (or a ratio that gives left and right fixed widths with center filling).
- Left section: render `app.view_mode.name()` left-aligned, styled Cyan.
- Center section: empty for now (STORY-064 adds breadcrumb).
- Right section: render `"? for help"` right-aligned, styled DarkGray.
- Use `│` separator between sections via Span composition within a single Line, or render each section independently.

Simpler approach: compose a single `Line` with left-aligned and right-aligned spans rendered into the full `outer[2]` area. Left spans: mode name. Right spans (via padding or dual Paragraph render): help hint. This avoids sub-layout splitting.

**How to verify:**
```
cargo run -- tui
```
Visually confirm the footer appears on Types, Filters, Metrics, Graph, and Agents screens. Verify left shows panel name, right shows "? for help".

### Task 2: Remove mode indicator from header

**ACs addressed:** AC-4 (header shows only "lazyspec")

**Files:**
- Modify: `src/tui/ui.rs` (lines 110-117 in `draw()`)

**What to implement:**

Delete the mode indicator rendering block:

```rust
// DELETE these lines (110-117):
let mode_indicator = Line::from(vec![Span::styled(
    format!("[{}] ` to cycle ", app.view_mode.name()),
    Style::default().fg(Color::DarkGray),
)]);
f.render_widget(
    Paragraph::new(mode_indicator).alignment(Alignment::Right),
    outer[0],
);
```

The header retains only the "lazyspec" title paragraph (lines 102-109).

**How to verify:**
```
cargo run -- tui
```
Confirm header shows only "lazyspec" with no mode indicator text on the right side.

## Test Plan

| # | AC | Test | Type | Tradeoffs |
|---|-----|------|------|-----------|
| 1 | AC-1 | Render the TUI in a test terminal backend, assert `outer` layout has 3 constraints (Length(1), Min(0), Length(1)) | Unit | Behavioural -- tests layout structure not pixel output |
| 2 | AC-2 | For each ViewMode variant, render and assert the status bar line contains the mode name string | Unit | Specific -- one assertion per mode |
| 3 | AC-3 | Render any screen, assert the status bar line contains "? for help" | Unit | Fast, deterministic |
| 4 | AC-4 | Render the header area, assert it does not contain "to cycle" or the mode name | Unit | Structure-insensitive -- checks absence of old content |

All tests use ratatui's `TestBackend` for deterministic rendering. No integration tests needed -- this is purely UI rendering logic.

## Notes

The Agents screen already has its own internal footer (`draw_agents_screen` lines 1279-1353). This iteration does NOT migrate it -- the Agents screen will temporarily have two footers (its internal one within the content area, plus the new global status bar). STORY-064 handles the migration. This is acceptable because the global footer provides different information (panel name + help hint) than the Agents footer (keybinding hints).
