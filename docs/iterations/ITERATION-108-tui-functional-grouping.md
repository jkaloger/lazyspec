---
title: TUI Functional Grouping
type: iteration
status: accepted
author: jkaloger
date: 2026-03-24
tags: []
related:
- blocks: ITERATION-107
---




Reorganise the TUI module tree from a flat layout into functional groups: `content/`, `views/`, `state/`, `infra/`. Extract the event loop from `tui.rs` into `infra/event_loop.rs`. Rename ambiguous `cache` modules. Addresses AUDIT-008 F5, F9, F10. Depends on ITERATION-107 (root-file migration) being complete.

## Changes

### Task 1: Create content/ group (gfm + diagram)

Files:
- Create: `src/tui/content.rs` (root file with `pub mod gfm; pub mod diagram;`)
- Move: `src/tui/gfm.rs` -> `src/tui/content/gfm.rs`
- Move: `src/tui/gfm/parse.rs` -> `src/tui/content/gfm/parse.rs`
- Move: `src/tui/gfm/render.rs` -> `src/tui/content/gfm/render.rs`
- Move: `src/tui/diagram.rs` -> `src/tui/content/diagram.rs`
- Modify: `src/tui.rs` (replace `pub mod gfm; pub mod diagram;` with `pub mod content;`)

Update all internal `crate::tui::gfm` and `crate::tui::diagram` references within `src/tui/` to use `crate::tui::content::gfm` and `crate::tui::content::diagram`. External test imports for `tui::diagram` (2 files: `tests/tui_diagram_test.rs`, `tests/tui_probe_test.rs`) must also be updated. `tui::gfm` has no external consumers.

Verify `cargo build --all-features` passes.

### Task 2: Create views/ group (ui + keys)

Files:
- Rename: `src/tui/ui.rs` -> `src/tui/views.rs`
- Rename: `src/tui/ui/` -> `src/tui/views/`
- Move: `src/tui/app/keys.rs` -> `src/tui/views/keys.rs`
- Modify: `src/tui/views.rs` (add `pub mod keys;`, remove keys from app module)
- Modify: `src/tui.rs` (replace `pub mod ui;` with `pub mod views;`)
- Modify: `src/tui/state/app.rs` or `src/tui/app.rs` (remove `mod keys;` declaration)

Update all `crate::tui::ui` references to `crate::tui::views` within `src/tui/`. External test imports for `tui::ui` (2 files: `tests/tui_image_sizing_test.rs`, `tests/tui_image_alignment_test.rs`) must also be updated. The `keys.rs` module currently uses `super::` paths referencing `App`; these need updating to `crate::tui::state::app::App` (or whatever the state path is after task 3).

> [!WARNING]
> Task 2 and task 3 are coupled: moving `keys.rs` out of `app/` means its `super::` imports break. Either do tasks 2 and 3 together, or temporarily use absolute `crate::` paths in `keys.rs` during task 2 and fix them in task 3.

Verify `cargo build --all-features` passes.

### Task 3: Create state/ group (app + forms + cache + graph)

Files:
- Create: `src/tui/state.rs` (root file with `pub mod app;` and re-exports)
- Move: `src/tui/app.rs` -> `src/tui/state/app.rs`
- Move: `src/tui/app/forms.rs` -> `src/tui/state/forms.rs`
- Move: `src/tui/app/cache.rs` -> `src/tui/state/cache.rs`
- Move: `src/tui/app/graph.rs` -> `src/tui/state/graph.rs`
- Modify: `src/tui.rs` (replace `pub mod app;` with `pub mod state;`)

Update all internal `crate::tui::app` references to `crate::tui::state` (or `crate::tui::state::app` for the `App` struct specifically). Update 22 test files that import from `lazyspec::tui::app::*` to use `lazyspec::tui::state::*`. The `state.rs` root file should re-export `App`, `AppEvent`, `ViewMode`, `FilterField`, `FormField`, `CreateForm`, `DeleteConfirm`, `StatusPicker`, `LinkEditor`, `AgentDialog`, `traverse_dependency_chain`, `PreviewTab`, `resolve_editor_from` so that external consumers can import from `tui::state::` directly.

Rename `src/tui/state/cache.rs` to `src/tui/state/expansion.rs` to resolve AUDIT-008 F5 (ambiguous cache names). Update the `mod` declaration accordingly.

Verify `cargo build --all-features` passes.

### Task 4: Create infra/ group (event loop + terminal_caps + perf_log)

Files:
- Create: `src/tui/infra.rs` (root file with `pub mod event_loop; pub mod terminal_caps; pub mod perf_log;`)
- Create: `src/tui/infra/event_loop.rs` (extracted from `src/tui.rs`)
- Move: `src/tui/terminal_caps.rs` -> `src/tui/infra/terminal_caps.rs`
- Move: `src/tui/perf_log.rs` -> `src/tui/infra/perf_log.rs`
- Modify: `src/tui.rs` (replace `pub mod terminal_caps; pub mod perf_log;` with `pub mod infra;`, move event loop logic into `infra::event_loop`, keep `tui.rs` as a thin router that re-exports `pub fn run()`)

Extract the event loop (~280 lines: terminal setup, channel creation, background thread spawning, render loop, teardown) from `tui.rs` into `infra/event_loop.rs`. Leave `tui.rs` as a thin module root with `pub mod` declarations and a `pub fn run()` that delegates to `infra::event_loop::run()`.

Update external test imports for `tui::terminal_caps` (2 files: `tests/tui_diagram_test.rs`, `tests/tui_probe_test.rs`) to `tui::infra::terminal_caps`.

Verify `cargo build --all-features` passes.

### Task 5: Final import sweep and cleanup

Files:
- Modify: all test files under `tests/tui_*.rs` (verify all import paths are correct)
- Remove: empty `src/tui/app/` directory, empty `src/tui/ui/` directory, empty `src/tui/gfm/` directory

Run `cargo test` (full suite). Grep for any remaining references to old paths (`tui::app::`, `tui::ui::`, `tui::gfm::`, `tui::diagram::`, `tui::terminal_caps::`, `tui::perf_log::`) in the entire codebase. Fix any stragglers. Remove empty directories left behind by the moves.

Verify the final directory tree matches the agreed layout from AUDIT-008.

## Test Plan

- `cargo build --all-features` passes after each task
- `cargo test` passes after task 5 (full suite)
- `grep -r 'tui::app::' src/ tests/` returns zero matches (old path fully removed)
- `grep -r 'tui::ui::' src/ tests/` returns zero matches
- `grep -r 'tui::gfm' src/ tests/` only matches `tui::content::gfm`
- `grep -r 'tui::diagram' src/ tests/` only matches `tui::content::diagram`
- Directory tree under `src/tui/` matches AUDIT-008 agreed layout
- No empty directories remain

Tradeoff: tasks 2 and 3 sacrifice isolation (moving `keys.rs` cross-module creates a dependency between them). This trades the Isolated test property for Predictive (doing them together ensures the final state is correct, rather than testing an intermediate state that will immediately change).

## Notes

Tasks 1-4 should be done in order since they progressively reshape the module tree. Task 5 is a cleanup sweep. Each task should be its own commit.

The `agent.rs` module stays at `src/tui/agent.rs` (top-level, not in any group). It is a domain concern, not infrastructure.

The biggest risk is task 3 (state/ group) since it touches 22 test files. The re-export pattern in `state.rs` can mitigate this: if `state.rs` re-exports all public symbols from `app.rs`, test files only need to change `tui::app::` to `tui::state::` (one token change per import line).
