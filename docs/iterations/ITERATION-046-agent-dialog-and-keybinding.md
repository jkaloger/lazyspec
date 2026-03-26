---
title: Agent dialog and keybinding
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-051
---




## Changes

Implements Group A of STORY-051: dialog struct, keybinding, input blocking, config-aware action filtering, and rendering. Execution logic (Group B) is out of scope.

### Task 1: AgentDialog struct and keybinding

**ACs addressed:** AC1, AC8
**Files:**
- Modify: `src/tui/app.rs`
- Test: `tests/tui_agent_dialog_test.rs`

**What to implement:**
Add `AgentDialog` struct with fields: `active: bool`, `selected_index: usize`, `actions: Vec<String>`, `doc_path: PathBuf`, `doc_title: String`. Add field `agent_dialog: AgentDialog` to `App`. In `handle_normal_key()`, on `KeyCode::Char('a')`: if `self.get_selected_doc()` returns `None` (AC8), do nothing. Otherwise, populate `agent_dialog` with the selected doc info, compute available actions (always include "Expand document", "Custom prompt"; include "Create children" only if the doc type has children per config rules), set `active = true`.

**How to verify:** `cargo test tui_agent_dialog`

### Task 2: Dialog key handling

**ACs addressed:** AC7, AC9
**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**
Add `handle_agent_dialog_key()` method. Insert check for `self.agent_dialog.active` in `handle_key()` dispatch chain, after `delete_confirm` and before `search_mode`. Key handling: `Esc` sets `active = false` (AC7). `Up`/`Down` cycle `selected_index`. `Enter` reads the selected action (stub: just close dialog for now, Group B adds execution). All other keys are ignored (AC9).

**How to verify:** `cargo test tui_agent_dialog`

### Task 3: Config-aware action filtering

**ACs addressed:** AC4
**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**
When building the actions list in Task 1's open logic, iterate `config.rules` for `ParentChild` variants. If any rule has `parent` matching the selected doc's type, include "Create children". If none match, omit it. This ensures leaf types (e.g., iteration) never show "Create children".

**How to verify:** `cargo test tui_agent_dialog`

### Task 4: Render agent dialog

**ACs addressed:** AC1
**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**
Add `draw_agent_dialog(f, app)` following the `draw_delete_confirm` pattern: centered popup (40% width, action count + chrome height), `Clear` background, rounded `Block` border titled "Agent Actions — {doc_title}". Render each action as a `ListItem`, highlight `selected_index` with reversed style. Call from `draw()` when `app.agent_dialog.active`, before help/warning overlays.

**How to verify:** Manual visual check via `cargo run` TUI. Automated tests cover state only.

### Task 5: Tests

**ACs addressed:** AC1, AC4, AC7, AC8, AC9
**Files:**
- Create: `tests/tui_agent_dialog_test.rs`

**What to implement:**
Using `TestFixture` and `setup_app_with_docs()` pattern:
- `test_a_key_opens_dialog`: select a doc, press `a`, assert `agent_dialog.active == true` and actions list is non-empty (AC1)
- `test_a_key_empty_list`: ensure empty doc list, press `a`, assert `agent_dialog.active == false` (AC8)
- `test_esc_closes_dialog`: open dialog, press `Esc`, assert `active == false` (AC7)
- `test_unhandled_key_ignored`: open dialog, press `x`, assert dialog still active and state unchanged (AC9)
- `test_no_create_children_for_iteration`: select an iteration doc, press `a`, assert "Create children" not in actions list (AC4)
- `test_create_children_for_rfc`: select an RFC doc, press `a`, assert "Create children" in actions list (AC4)

**How to verify:** `cargo test tui_agent_dialog`

## Test Plan

- `test_a_key_opens_dialog` — AC1: dialog opens with actions for selected doc
- `test_a_key_empty_list` — AC8: no dialog when doc list empty
- `test_esc_closes_dialog` — AC7: Esc dismisses dialog cleanly
- `test_unhandled_key_ignored` — AC9: random keys don't leak through
- `test_no_create_children_for_iteration` — AC4: leaf types omit "Create children"
- `test_create_children_for_rfc` — AC4: parent types include "Create children"

## Notes

Group B (AC2, AC3, AC5, AC6, AC10) covers actual agent execution and is a separate iteration. Task 2's Enter handler is a stub that closes the dialog until Group B lands.
