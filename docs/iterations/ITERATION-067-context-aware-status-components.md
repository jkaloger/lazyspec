---
title: Context-aware status components
type: iteration
status: draft
author: agent
date: 2026-03-13
tags: []
related:
- implements: docs/stories/STORY-064-context-aware-status-components.md
---


## Changes

### Task 1: Add git branch to App and status bar left section

**ACs addressed:** AC-1 (git branch in left section)

**Files:**
- Modify: `src/tui/app.rs` (App struct + constructor)
- Modify: `src/tui/mod.rs` (run function, populate branch at startup)
- Modify: `src/tui/ui.rs` (draw_status_bar)

**What to implement:**

Add `pub git_branch: Option<String>` field to the `App` struct (after `view_mode`, line ~300). Initialize it as `None` in `App::new`.

In `run()` (src/tui/mod.rs), after `App::new` and before the event loop, run `git rev-parse --abbrev-ref HEAD` via `std::process::Command` and store the trimmed output in `app.git_branch`. If the command fails (not a git repo), leave it as `None`.

In `draw_status_bar` (src/tui/ui.rs), extend the left section. After the mode name span, if `app.git_branch` is `Some(ref branch)`, append a separator and the branch name styled DarkGray. Update the padding calculation to account for the branch text.

**How to verify:**
```
cd /some/git/repo && cargo run -- tui
# Left section shows: "Types │ main" (or current branch)
cd /tmp && cargo run -- tui
# Left section shows: "Types │" (no branch)
```

### Task 2: Add parent breadcrumb to status bar center section

**ACs addressed:** AC-2 (breadcrumb shown), AC-3 (empty when no relationships)

**Files:**
- Modify: `src/tui/ui.rs` (draw_status_bar)

**What to implement:**

In `draw_status_bar`, compute the breadcrumb for the currently selected document. Use the existing chain-walk pattern from `app.relation_items()` (app.rs:914): get `app.selected_doc_meta()`, then walk `Implements` links upward via `app.store.forward_links` to find the immediate parent.

If a parent exists, format the breadcrumb as `"PARENT-ID > DOC-ID"` (extracting the document ID from the filename, e.g. `RFC-021 > STORY-064`). Render this in the center padding area, styled White. If no parent relationship exists, leave the center empty (current behavior).

Only show the immediate parent (one level up), not the full chain. Extract the ID from the path stem by taking the portion before the first `-` that follows the document type prefix (e.g. `STORY-064-context-aware-status-components` becomes `STORY-064`).

**How to verify:**
```
cargo run -- tui
# Select a Story that implements an RFC -- center shows "RFC-021 > STORY-064"
# Select an RFC with no parent -- center is empty
# Select an Iteration -- center shows "STORY-063 > ITERATION-066"
```

### Task 3: Context-sensitive right section and Agents footer migration

**ACs addressed:** AC-4 (Agents keybinding hints in right section), AC-5 (remove bespoke footer)

**Files:**
- Modify: `src/tui/ui.rs` (draw_status_bar, draw_agents_screen)

**What to implement:**

In `draw_status_bar`, make the right section context-sensitive. When `app.view_mode` is `ViewMode::Agents` (behind `#[cfg(feature = "agent")]`), render the keybinding hints instead of "? for help". The hints are: `e: open  r: resume  `: switch view` -- matching the current Agents footer styling (key in Cyan, description in DarkGray).

In `draw_agents_screen` (lines 1299-1374), remove the internal footer layout split. Change the layout from a 2-constraint split (`[Min(0), Length(1)]`) to just rendering the content into the full `area` directly. Delete the `footer_area`, the `footer` Line, and the `Paragraph::new(footer)` render call (lines 1301-1373). The table/empty-state should render into `area` instead of `main_area`.

**How to verify:**
```
cargo run -- tui
# Navigate to Agents screen (backtick to cycle)
# Right section shows "e: open  r: resume  `: switch view" instead of "? for help"
# No duplicate footer inside the Agents content area
# Switch back to Types -- right section shows "? for help" again
```

## Test Plan

| # | AC | Test | Type | Tradeoffs |
|---|-----|------|------|-----------|
| 1 | AC-1 | Construct App with git_branch = Some("main"), render status bar into TestBackend, assert output contains "main" after mode name | Unit | Behavioral -- checks rendered text, not span structure |
| 2 | AC-1 | Construct App with git_branch = None, render status bar, assert no branch text appears after separator | Unit | Specific -- failure pinpoints missing None handling |
| 3 | AC-2 | Set up store with Story implementing RFC, select the Story, render status bar, assert center contains "RFC-XXX > STORY-YYY" | Unit | Predictive -- tests actual graph traversal + rendering |
| 4 | AC-3 | Select a document with no parent relationships, render status bar, assert center section is empty (only whitespace) | Unit | Structure-insensitive -- checks absence of breadcrumb text |
| 5 | AC-4 | Set view_mode to Agents, render status bar, assert right section contains "e: open" and "r: resume" | Unit | Behavioral -- tests content, not span styling |
| 6 | AC-4 | Set view_mode to Types, render status bar, assert right section contains "? for help" | Unit | Fast, deterministic baseline |
| 7 | AC-5 | Render draw_agents_screen, assert no internal footer layout (area is passed through without split) | Unit | Structure-insensitive -- verifies removal, not pixel layout |

All tests use ratatui's `TestBackend`. No integration tests needed.

## Notes

The `relation_items` method (app.rs:914) already walks the full `Implements` chain upward. For the breadcrumb we only need one level, so a simpler lookup via `store.forward_links` on the selected doc is sufficient -- no need to call `relation_items` directly.

The Agents screen will temporarily lose its internal footer before the status bar right section gains the keybinding hints. Task 3 handles both sides together to avoid this gap.
