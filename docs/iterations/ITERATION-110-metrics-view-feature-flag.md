---
title: "Metrics view feature flag"
type: iteration
status: accepted
author: "agent"
date: 2026-03-26
tags: []
related: []
---


Gate `ViewMode::Metrics` and `draw_metrics_skeleton` behind a `metrics` cargo feature flag, following the pattern established by the `agent` feature flag. Default builds will exclude the metrics view entirely, satisfying the YAGNI concern in STORY-036 while preserving the code for future implementation (STORY-014).

## Changes

### Task 1: Add `metrics` feature flag to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

**What to implement:**
Add `metrics = []` to the `[features]` section, alongside the existing `agent = []` entry.

**How to verify:**
`cargo check` passes. `cargo check --features metrics` passes.

### Task 2: Gate `ViewMode::Metrics` enum variant and match arms

**Files:**
- Modify: `src/tui/state/app.rs`

**What to implement:**
Add `#[cfg(feature = "metrics")]` to the `Metrics` variant at line 131.

In `next()` (lines 138-150):
- Gate `ViewMode::Filters => ViewMode::Metrics` with `#[cfg(feature = "metrics")]`
- Gate `ViewMode::Metrics => ViewMode::Graph` with `#[cfg(feature = "metrics")]`
- Add `#[cfg(not(feature = "metrics"))]` arm: `ViewMode::Filters => ViewMode::Graph`

In `name()` (lines 152-161):
- Gate `ViewMode::Metrics => "Metrics"` with `#[cfg(feature = "metrics")]`

Follow the exact same pattern used for `ViewMode::Agents` with the `agent` feature.

**How to verify:**
`cargo check` (no features) passes with no warnings. `cargo check --features metrics` passes. `cargo check --all-features` passes.

### Task 3: Gate `draw_metrics_skeleton` import and call site in views.rs

**Files:**
- Modify: `src/tui/views.rs`

**What to implement:**
At line 27, move `draw_metrics_skeleton` out of the shared `use panels::{...}` import and into a separate `#[cfg(feature = "metrics")]` import, following the same pattern as `draw_agents_screen` at lines 30-31.

At line 105, gate the match arm `ViewMode::Metrics => draw_metrics_skeleton(f, outer[1])` with `#[cfg(feature = "metrics")]`.

**How to verify:**
`cargo check` (no features) passes. `cargo check --features metrics` passes.

### Task 4: Gate `draw_metrics_skeleton` function in panels.rs

**Files:**
- Modify: `src/tui/views/panels.rs`

**What to implement:**
Add `#[cfg(feature = "metrics")]` to the `draw_metrics_skeleton` function definition at line 965.

**How to verify:**
`cargo check --features metrics` passes. Without the feature, the function is excluded and no dead-code warning appears.

### Task 5: Update test assertions for metrics-less cycle

**Files:**
- Modify: `tests/tui_view_mode_test.rs`
- Modify: `tests/tui_graph_test.rs`
- Modify: `tests/tui_agent_management_test.rs`

**What to implement:**

In `tests/tui_view_mode_test.rs`:
- `test_view_mode_next_cycles` (line 31): gate the two Metrics assertions (lines 33-34) with `#[cfg(feature = "metrics")]`. Add `#[cfg(not(feature = "metrics"))]` assertion: `Filters.next() == Graph`.
- `test_backtick_cycles_mode` (line 44): the third backtick press at line 53 currently expects `Metrics`. Gate that assertion with `#[cfg(feature = "metrics")]`. Add `#[cfg(not(feature = "metrics"))]` assertion expecting `Graph` instead.

In `tests/tui_graph_test.rs`:
- `test_graph_rebuilds_on_mode_switch` (line 135): the cycle currently takes 3 backtick presses to reach Graph (Types->Filters->Metrics->Graph). Without the metrics feature, it takes 2 presses (Types->Filters->Graph). Gate the third key press (line 142) and `Metrics` assertion (line 143) with `#[cfg(feature = "metrics")]`. Similarly adjust the return cycle at lines 160-163.

In `tests/tui_agent_management_test.rs`:
- `test_agents_view_mode_in_cycle` (line 43): gate the two Metrics assertions (lines 45-46) with `#[cfg(feature = "metrics")]`. Add `#[cfg(not(feature = "metrics"))]` assertion: `Filters.next() == Graph`.

**How to verify:**
`cargo test` (no features) passes. `cargo test --features metrics` passes. `cargo test --all-features` passes.

## Test Plan

- `cargo check` (no features): compiles without `ViewMode::Metrics` or `draw_metrics_skeleton`
- `cargo check --features metrics`: compiles with metrics code included
- `cargo check --all-features`: compiles with both metrics and agent features
- `cargo test` (no features): all view-mode cycle tests pass, asserting Filters->Graph skip
- `cargo test --features metrics`: all view-mode cycle tests pass, asserting Filters->Metrics->Graph
- `cargo test --all-features`: full cycle including both Metrics and Agents

All tests are deterministic and structure-insensitive (they test the cycle behavior, not the rendering output). No new test files needed; existing tests are updated to handle both feature configurations.

## Notes

Follows the exact precedent set by ITERATION-048 (agent feature flag). The `#[cfg(feature)]` / `#[cfg(not(feature))]` pattern on `next()` match arms is the critical piece, ensuring the mode cycle skips Metrics when the feature is off.
