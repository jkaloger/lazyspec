---
title: Config ASCII preference and nesting refactor
type: iteration
status: accepted
author: agent
date: 2026-03-14
tags: []
related:
- implements: docs/stories/STORY-063-diagram-rendering-pipeline.md
---



## Changes

### Task 1: Add `ascii_diagrams` config option

**ACs addressed:** AC8 (fallback when terminal lacks image support)

**Files:**
- Modify: `src/engine/config.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`

**What to implement:**

Add a `[tui]` section to `.lazyspec.toml` with an `ascii_diagrams` boolean:

```toml
[tui]
ascii_diagrams = true
```

1. In `src/engine/config.rs`:
   - Add a `Tui` struct with `pub ascii_diagrams: bool`, implementing `Default` (defaults to `false`).
   - Add `pub tui: Tui` to `Config` (line 42).
   - Add `tui: Option<Tui>` to `RawConfig` (line 71).
   - In `Config::parse`, assign `tui: raw.tui.unwrap_or_default()`.

2. In `src/tui/app.rs`:
   - Store `ascii_diagrams: bool` on `App`, read from `config.tui.ascii_diagrams` in `App::new`.

3. In `src/tui/ui.rs`:
   - In `draw_preview_content` and `draw_fullscreen`, when `app.ascii_diagrams` is true, skip image protocol rendering entirely and fall back to the syntax-highlighted code block with a hint: `[diagram: ASCII mode enabled in config]`.
   - This check should be early in the diagram rendering path, before any image loading or protocol work.

**How to verify:**
```
cargo test
```
Set `ascii_diagrams = true` in `.lazyspec.toml`, run `cargo run`, confirm diagrams render as syntax-highlighted code blocks instead of images.

---

### Task 2: Flatten nesting in `ui.rs` preview functions

**ACs addressed:** N/A (refactor)

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Three refactors in `ui.rs`:

1. `draw_preview_content` (line 391): Replace `if let Some(doc) = doc { ... } else { ... }` wrapping the entire function body with an early return:
   ```rust
   let Some(doc) = doc else {
       // render empty state
       return;
   };
   ```
   This eliminates one nesting level for ~150 lines.

2. `draw_fullscreen` (line 705): Same pattern at line 714. Apply the same early-return refactor.

3. Extract a shared `render_image_overlay` helper function that both `draw_preview_content` (lines 497-541) and `draw_fullscreen` (lines 809-829) call. The current image overlay blocks are near-identical with 4-6 levels of nesting. The helper takes `(frame, app, hash, path, img_area)` and handles the image loading, state creation, and rendering.

**How to verify:**
```
cargo test
cargo run -- show docs/rfcs/RFC-017-better-markdown-preview.md
```
All existing tests pass. Visual rendering unchanged.

---

### Task 3: Flatten nesting in `app.rs` key handlers

**ACs addressed:** N/A (refactor)

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Two refactors in `app.rs`:

1. `handle_normal_key` (line 1579): The function has sequential `if self.view_mode == ViewMode::Filters { ... return; }` and `if self.view_mode == ViewMode::Graph { ... return; }` blocks before the main match. Extract these into `handle_filters_key(&mut self, code, modifiers)` and `handle_graph_key(&mut self, code, modifiers)` methods (matching the existing `handle_agents_key` pattern at line 1527). Dispatch via a top-level match on `self.view_mode`.

2. `handle_agent_dialog_key` (line 1378): The `KeyCode::Enter` arm (lines 1401-1442) has 5 levels of nesting for the "Create children" action. Extract a `spawn_create_children(&mut self, config)` helper that uses early returns to flatten the logic.

**How to verify:**
```
cargo test
```
All existing key handling tests pass unchanged.

## Test Plan

### T1: Config parsing with `[tui]` section

Unit test that parses a TOML string containing `[tui]\nascii_diagrams = true` and asserts `config.tui.ascii_diagrams == true`. A second case omitting the section asserts the default is `false`.

- Properties: Isolated, Deterministic, Fast, Specific

### T2: Config missing `[tui]` section uses defaults

Unit test that parses a minimal TOML (no `[tui]` section) and asserts `config.tui.ascii_diagrams == false`.

- Properties: Isolated, Deterministic, Fast, Specific
- Note: Can be combined with T1 as two cases in one test function.

### T3: ASCII mode skips image rendering

Test that when `app.ascii_diagrams` is true and a diagram block is present, the preview output contains the fallback hint text rather than image overlay data.

- Properties: Isolated, Deterministic, Fast, Behavioral
- Tradeoff: Tests behavior at the segment-building level rather than full frame rendering. Sacrifices Predictive slightly for Fast.

### T4: Refactoring doesn't change behavior

No new tests needed for Tasks 2-3. The existing test suite (`tests/tui_*.rs`) covers the key handling and rendering paths being refactored. All existing tests passing is the verification.

- Properties: Structure-insensitive (the whole point)

## Notes

Tasks 2 and 3 are pure refactors with no behavior change. They should be done after ITERATION-069 lands, since that iteration modifies the same image rendering code paths. Applying these refactors second avoids merge conflicts and means the refactored code is already in its final shape.
