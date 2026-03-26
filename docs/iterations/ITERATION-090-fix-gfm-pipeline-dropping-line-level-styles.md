---
title: Fix GFM pipeline dropping line-level styles
type: iteration
status: draft
author: agent
date: 2026-03-19
tags: []
related:
- implements: STORY-067
---


## Context

`render_gfm_segments` in `src/tui/gfm.rs` converts `tui_markdown` output into
owned `Line` instances. The conversion copies per-span styles but drops the
`Line.style` field. `tui-markdown` applies heading styles (bold, cyan,
underlined, etc. via `DefaultStyleSheet`) at the line level, so headings render
unstyled.

## Changes

### Task 1: Preserve line-level style in GFM rendering

**File:** `src/tui/gfm.rs`, `render_gfm_segments` function (~line 423)

Change `Line::from(owned_spans)` to `Line::from(owned_spans).style(line.style)`
so that line-level styles from `tui_markdown` are carried through.

### Task 2: Add regression test

**File:** `src/tui/gfm.rs`, tests module

Add a test that renders `# Heading\n\nBody text.\n` through the GFM pipeline
and asserts the heading line carries the expected `DefaultStyleSheet` H1 style
(bold, underlined, on_cyan). Compare against direct `tui_markdown::from_str`
output to ensure parity.

## Test Plan

- `cargo test gfm` passes
- New regression test asserts line-level styles match direct `tui_markdown` output
- Manual: `cargo run` the TUI, confirm headings display styled (bold/cyan)
