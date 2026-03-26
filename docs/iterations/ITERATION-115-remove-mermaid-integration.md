---
title: Remove mermaid integration
type: iteration
status: accepted
author: agent
date: 2026-03-25
tags: []
related:
- implements: docs/stories/STORY-063-diagram-rendering-pipeline.md
---



## Context

Remove the `Mermaid` variant and all mermaid/mmdc code paths from the diagram module. D2 support remains unchanged. This clears the way for a future `mermaid-rs-renderer` integration that won't shell out to `mmdc`.

No Cargo.toml changes required -- mermaid was pure CLI shelling with no Rust crate dependencies.

## Changes

### Task 1: Remove Mermaid from DiagramLanguage enum and tool functions

**Files:**
- Modify: `src/tui/content/diagram.rs`

**What to implement:**

1. Remove the `Mermaid` variant from `DiagramLanguage` enum (line 16)
2. Remove the `Mermaid` match arm from `tool_name()` (line 29)
3. Remove the `Mermaid` check from `is_tool_available()` -- this function now only handles D2, so `tool_name` will only return `"d2"`. No changes needed beyond the enum removal (the match becomes exhaustive with just D2).
4. In `extract_diagram_blocks()`, remove the `else if trimmed == "```mermaid"` branch (lines 65-66). Mermaid fenced blocks will now be ignored like any other non-d2 code block.

**How to verify:**
`cargo test test_extract` -- d2 extraction tests still pass, mermaid blocks are no longer extracted.

### Task 2: Simplify ToolAvailability to d2-only

**Files:**
- Modify: `src/tui/content/diagram.rs`
- Modify: `src/tui/state/app.rs`

**What to implement:**

1. In `ToolAvailability` struct (line 114-117), remove the `mmdc: bool` field. Keep only `d2: bool`.
2. In `ToolAvailability::detect()` (lines 120-125), remove the `mmdc` field initialization.
3. In `ToolAvailability::is_available()` (lines 127-132), remove the `Mermaid` match arm. With only the `D2` variant, the match simplifies to returning `self.d2`.
4. In `src/tui/state/app.rs`, update the two `ToolAvailability` initializations at lines 307 and 1359 to remove `mmdc: false`.

**How to verify:**
`cargo test test_tool_availability` -- availability tests pass with simplified struct.

### Task 3: Remove Mermaid rendering paths

**Files:**
- Modify: `src/tui/content/diagram.rs`

**What to implement:**

1. In `render_diagram()` (lines 147-200), remove the `DiagramLanguage::Mermaid` match arm (lines 167-179). The match on `block.language` now only has the D2 arm.
2. In `render_diagram_text()` (lines 202-228), remove the early-return mermaid bail (lines 203-205). With `Mermaid` gone from the enum, this guard is dead code.

**How to verify:**
`cargo test test_render_diagram` -- d2 rendering tests still pass.

### Task 4: Update test files

**Files:**
- Modify: `tests/tui_diagram_test.rs`
- Modify: `tests/tui_probe_test.rs`

**What to implement:**

In `tests/tui_diagram_test.rs`:
1. Remove `test_extract_mermaid_block` (lines 18-26)
2. Update `test_extract_multiple_blocks` (lines 28-35) -- change to use two d2 blocks, or remove if redundant with `test_extract_d2_block`
3. Remove `test_tool_name_mermaid` (lines 56-59)
4. Remove `test_tool_availability_is_available_mermaid` (lines 104-109)
5. Update `test_tool_availability_is_available_d2` (lines 97-102) -- remove the `mmdc: false` field and the `Mermaid` assertion
6. Remove `test_render_diagram_text_mermaid_errors` (lines 159-170)
7. Update all `ToolAvailability` constructors in remaining tests to remove `mmdc` field (lines 231, 250, 263, 277, 293)

In `tests/tui_probe_test.rs`:
1. Remove `assert!(!app.tool_availability.mmdc)` (line 25)
2. Update `ToolAvailability { d2: true, mmdc: false }` to `ToolAvailability { d2: true }` (line 49)
3. Remove `assert!(!tool_availability.mmdc)` (line 58)

**How to verify:**
`cargo test` -- full test suite passes with no mermaid references.

## Test Plan

All changes are removals, so the test strategy is regression-focused: verify that d2 functionality remains intact after mermaid code is stripped.

- `test_extract_d2_block` -- d2 block extraction unchanged
- `test_extract_no_diagram_blocks` -- non-diagram blocks still ignored
- `test_extract_nested_backticks` -- 4+ backtick fences still skipped
- `test_tool_name_d2` -- d2 tool name unchanged
- `test_tool_availability_is_available_d2` -- d2 availability detection works
- `test_source_hash_deterministic` -- hashing unchanged
- `test_render_diagram_produces_png` -- d2 rendering still produces PNG (requires d2 installed)
- `test_render_diagram_text_produces_ascii` -- d2 ASCII fallback still works (requires d2 installed)
- `test_diagram_cache_*` -- caching unchanged
- `test_build_segments_*` -- preview segment building works with d2-only ToolAvailability
- `app_new_returns_within_100ms_with_halfblock_picker` -- app init with simplified ToolAvailability
- `probe_result_updates_app_state` -- probe result handling with d2-only struct

No new tests needed. The removal should cause no behavioral change for d2 users. Mermaid fenced blocks will render as plain code (syntax-highlighted), which is the existing fallback behavior when mmdc is not installed.

## Notes

This is preparation for replacing the mmdc CLI shelling with `mermaid-rs-renderer`, a Rust-native mermaid renderer. The future integration will reintroduce the `Mermaid` variant with a different rendering backend.
