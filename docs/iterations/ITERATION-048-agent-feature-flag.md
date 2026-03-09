---
title: "Agent feature flag"
type: iteration
status: draft
author: "agent"
date: 2026-03-09
tags: []
related: []
validate_ignore: true
---

## Summary

Add a Cargo feature flag `agent` (disabled by default) to gate all TUI agent
functionality behind conditional compilation. When the flag is off, the binary
compiles without any agent spawning, dialog, management, or resume capability.

## Changes

### Task 1: Add feature flag to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

**What to implement:**
Add a `[features]` section with `default = []` and `agent = []`. The agent
feature has no extra dependencies since the spawning logic uses `std::process::Command`.

**How to verify:**
`cargo check` passes. `cargo check --features agent` passes.

---

### Task 2: Gate the agent module

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**
Add `#[cfg(feature = "agent")]` to `pub mod agent;` declaration.

**How to verify:**
`cargo check` passes without the feature (module excluded).

---

### Task 3: Gate agent usage in App struct and initialization

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Wrap with `#[cfg(feature = "agent")]`:
- The import line: `use crate::tui::agent::{load_all_records, AgentSpawner, AgentStatus};`
- The `AgentDialog` struct definition and its `new()` impl
- The `Agents` variant in `ViewMode` enum, plus its arms in `next()` and `name()`
- App struct fields: `agent_dialog`, `agent_spawner`, `agent_selected_index`, `resume_request`
- Their initialization in `App::new()` and in the test helper
- The `cycle_mode()` block that loads agent records
- The three agent key handler methods: `handle_agent_dialog_key`, `handle_agent_text_input_key`, `handle_agents_key`
- The match arm routing to `handle_agents_key` in the main key handler
- The 'a' key handler that opens the agent dialog
- The `agent_dialog.active` check that intercepts keys

**How to verify:**
`cargo check` passes without feature. `cargo check --features agent` passes with feature.

---

### Task 4: Gate agent usage in UI rendering

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Wrap with `#[cfg(feature = "agent")]`:
- The import: `use crate::tui::agent::AgentStatus;`
- The `draw_agent_dialog()` function and its call site
- The `draw_agents_screen()` function and its match arm in `draw()`

**How to verify:**
`cargo check` passes without feature. `cargo check --features agent` passes with feature.

---

### Task 5: Gate agent code in TUI event loop

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

Wrap with `#[cfg(feature = "agent")]`:
- `app.agent_spawner.poll_finished();` (line 93)
- The `resume_request` block (lines 103-117)

**How to verify:**
`cargo check` passes without feature.

---

### Task 6: Gate test files

**Files:**
- Modify: `tests/tui_agent_test.rs`
- Modify: `tests/tui_agent_management_test.rs`
- Modify: `tests/tui_agent_dialog_test.rs` (if it exists)

**What to implement:**
Add `#![cfg(feature = "agent")]` as the first line of each test file so the
entire file is excluded when the feature is off.

**How to verify:**
`cargo test` passes without feature (agent tests skipped).
`cargo test --features agent` passes with feature (agent tests run).

## Test Plan

- `cargo check` compiles without any features (agent code excluded)
- `cargo check --features agent` compiles with agent code included
- `cargo test` passes without the feature (no agent test failures)
- `cargo test --features agent` runs agent tests and they pass

These are all fast, deterministic, and structure-insensitive. No new test code
is needed; the existing agent tests verify correctness when the feature is on.

## Notes

The `ViewMode` enum needs care: when agent is disabled, the `Agents` variant
must not exist, and `next()` must skip from `Graph` directly to `Types`.
This requires cfg on the enum variant and on the relevant match arms.
