---
title: Diagram rendering bug fixes
type: iteration
status: accepted
author: agent
date: 2026-03-14
tags: []
related:
- implements: STORY-063
---




## Changes

### Task 1: Fix image vertical alignment in preview panel

**ACs addressed:** AC4 (inline image display)

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

The image overlay positioning in `draw_preview_content` (around line 497-540) has two bugs:

1. `y_offset` starts at 0 but the preview content has header lines above the segments (title, type/status/author, date, tags, blank line). The image overlay is placed at `inner.y + y_offset`, ignoring these ~5-6 header lines. Fix: calculate the number of header lines rendered before the markdown body and add that offset to `y_offset` initialization.

2. `y_offset` accumulates raw `md.lines.len()` for Markdown segments, but `Paragraph` wraps long lines at `content_width`. This causes images to appear too high when any preceding markdown has long lines. Fix: compute wrapped line counts using `content_width` instead of raw line counts. For each markdown line, calculate `ceil(line_display_width / content_width)` to get the actual number of rendered lines.

The same two issues exist in `draw_fullscreen` (around line 745-829). The fullscreen path correctly captures the line index at image insertion time, but still uses pre-wrap line counts. Apply the same wrap-aware counting fix there.

Additionally, when an image is scrolled past in fullscreen mode, `saturating_sub` clamps `scrolled_y` to 0, rendering the image at the top of the viewport instead of hiding it. Fix: skip rendering the image entirely when `line_y < app.scroll_offset`.

**How to verify:**
```
cargo test
cargo run -- show docs/rfcs/RFC-017-better-markdown-preview.md
```
Visually confirm images appear directly below their corresponding diagram code block position, not overlapping header text. Scroll past an image and confirm it disappears rather than sticking to the top.

---

### Task 2: Make terminal capability probe non-blocking

**ACs addressed:** AC1 (terminal image protocol detection)

**Files:**
- Modify: `src/tui/terminal_caps.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/mod.rs`

**What to implement:**

Currently `terminal_caps::create_picker()` calls `Picker::from_query_stdio()` synchronously at startup (`mod.rs:96`), blocking for up to 2 seconds. Additionally, `App::new()` calls `ToolAvailability::detect()` (`app.rs:404`) which runs `d2 --version` and `mmdc --version` as blocking subprocesses.

Move both probes to a background thread:

1. In `mod.rs`, spawn a thread before entering the event loop that runs both `create_picker()` and `ToolAvailability::detect()`.
2. Initialize `App` with `TerminalImageProtocol::None` and a default `ToolAvailability` (both tools unavailable) as the initial state, plus a `Picker` created via `Picker::halfblocks()` (instant, no IO).
3. Pass the probe result back through a channel (`std::sync::mpsc` or a shared `Arc<Mutex<Option<...>>>`).
4. In the event loop, check for the probe result each tick. When it arrives, update `app.terminal_image_protocol`, `app.tool_availability`, and `app.picker`. Trigger a redraw so any visible diagrams re-render with the correct protocol.

This ensures the TUI appears instantly with halfblock fallback rendering, then upgrades to sixel/kitty once the probe completes.

**How to verify:**
```
cargo test
time cargo run
```
Confirm the TUI appears instantly (no perceptible delay). After ~1-2s, diagrams should upgrade from placeholder/halfblock to full image protocol rendering.

---

### Task 3: Fix image resolution and size

**ACs addressed:** AC4 (inline image display)

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/diagram.rs`

**What to implement:**

Two issues with image rendering quality:

**Low resolution:** The `d2` and `mmdc` CLI invocations in `diagram.rs` need explicit DPI/scale flags to produce higher-resolution output. For `d2`, pass `--scale 2` (or `-s 2`). For `mmdc`, pass `-s 2` for a 2x scale factor. Check the current CLI invocation in `diagram.rs` and add these flags.

**Images not large enough:** The image display height is hardcoded to 12 lines (preview) and 15 lines (fullscreen) in `ui.rs`. Instead, derive the image height from the available width and the image's aspect ratio:
- Read the image dimensions from the PNG (the `image` crate or `ratatui-image` may already expose this).
- Calculate `image_height = (image_native_height / image_native_width) * available_width_in_cells`.
- Clamp to a sensible maximum (e.g., 80% of the panel height) to prevent a single diagram from consuming the entire viewport.
- Use the actual rendered height for the placeholder blank lines so alignment stays correct.

**How to verify:**
```
cargo test
cargo run -- show docs/rfcs/RFC-017-better-markdown-preview.md
```
Confirm diagrams render at higher resolution (not pixelated) and fill a reasonable portion of the preview panel width.

## Test Plan

### T1: Image vertical alignment

Write a test that constructs a preview with known header content and a diagram image segment, then asserts the image overlay `Rect` starts at the correct y-coordinate (header_lines + preceding_markdown_lines, accounting for wraps). This is a unit test on the offset calculation logic.

- Properties: Isolated, Deterministic, Fast, Specific, Structure-insensitive
- Tradeoff: Tests the offset calculation in isolation rather than full rendering. Sacrifices Predictive slightly (doesn't test actual ratatui rendering) for Fast and Isolated.

### T2: Image hidden when scrolled past

Test that when `scroll_offset > image_line_y`, the image is not included in the render output. Unit test on the scroll-clamp logic.

- Properties: Isolated, Deterministic, Fast, Specific

### T3: Non-blocking startup

Test that `App::new()` returns immediately (within a tight time bound, e.g. 100ms) when given a default/halfblock picker. The probe thread logic is tested separately by verifying the channel receives a result.

- Properties: Fast, Deterministic, Specific
- Tradeoff: Cannot easily test real stdio probing in CI (no real terminal). Tests the architecture (non-blocking init + channel update) rather than the probe itself.

### T4: Image dimensions derive from aspect ratio

Test that given an image with known dimensions and a known panel width, the calculated `image_height` matches the expected value. Unit test on the sizing function.

- Properties: Isolated, Deterministic, Fast, Specific, Structure-insensitive

## Notes

The `ratatui-image` crate's `Picker::from_query_stdio()` has a hardcoded 2s timeout. There's no way to reduce this without forking. The non-blocking approach sidesteps the issue entirely by running it off the main thread.

The existing test files (`tests/tui_*.rs`) will need updates since `App::new()` signature changes (Task 2). The picker parameter becomes optional or is replaced by a channel-based approach. Existing tests that construct `App` directly should use the instant `Picker::halfblocks()` path.
