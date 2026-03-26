---
title: Fix Ghostty detection and cache tool availability
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: STORY-063
---




## Context

Two bugs found after ITERATION-065 shipped:

1. Ghostty terminal (which supports the Kitty graphics protocol) isn't in the detection list, so it falls through to `Unsupported`.
2. `inject_fallback_hints` calls `is_tool_available` synchronously on every render frame. This spawns `d2 --version` or `mmdc --version` as a subprocess each time a document with diagram blocks is displayed. Unlike the ref expansion pattern (which uses background threads + crossbeam channel), this blocks the UI thread and causes visible lag or errors.

**ACs addressed:** AC1 (hardening), AC7 (performance fix)

## Changes

### Task 1: Add Ghostty to terminal detection

**ACs addressed:** AC1

**Files:**
- Modify: `src/tui/terminal_caps.rs`
- Modify: `tests/tui_diagram_test.rs`

**What to implement:**

Add Ghostty to `detect_from()`: `TERM_PROGRAM=ghostty` maps to `KittyGraphics`. Ghostty implements the Kitty graphics protocol.

**How to verify:**
```
cargo test --test tui_diagram_test
```

### Task 2: Cache tool availability check at startup

**ACs addressed:** AC7

**Files:**
- Modify: `src/tui/diagram.rs` (add `ToolAvailability` struct with cached results)
- Modify: `src/tui/app.rs` (store cached results in App, compute once at startup)
- Modify: `src/tui/ui.rs` (pass cached availability to `inject_fallback_hints`)

**What to implement:**

The tool availability check (`d2 --version`, `mmdc --version`) should run once at App startup, not on every render frame. Introduce a struct that caches the results:

```rust
pub struct ToolAvailability {
    pub d2: bool,
    pub mmdc: bool,
}

impl ToolAvailability {
    pub fn detect() -> Self { ... }
    pub fn is_available(&self, lang: &DiagramLanguage) -> bool { ... }
}
```

Add a `tool_availability: ToolAvailability` field to `App`, populated in `App::new()`.

Change `inject_fallback_hints` signature to accept `&ToolAvailability` instead of calling `is_tool_available` internally. Update both call sites in `ui.rs`.

**How to verify:**
```
cargo test --test tui_diagram_test
cargo test
```

## Test Plan

### T1: Ghostty detection

- `test_detect_ghostty_protocol` -- `detect_from(Some("ghostty"), None)` returns `KittyGraphics`

Pure function test. Fast, isolated, deterministic.

### T2: Tool availability caching

- `test_tool_availability_is_available_d2` -- construct `ToolAvailability { d2: true, mmdc: false }`, assert `is_available(&D2) == true`, `is_available(&Mermaid) == false`
- `test_tool_availability_is_available_mermaid` -- inverse

Pure struct tests. The actual `detect()` call depends on system state and isn't directly tested (same approach as `is_tool_available`).

## Notes

- Ghostty's Kitty protocol support is well documented. Other terminals to consider in future: foot, Konsole (both support Sixel).
- The startup cost of two subprocess spawns (`d2 --version` + `mmdc --version`) is negligible and only happens once.
