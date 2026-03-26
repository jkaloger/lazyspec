---
title: Status picker overlay
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-016
---




## Changes

### Task 1: StatusPicker struct and App integration

**ACs addressed:** AC1, AC5

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a `StatusPicker` struct following the `DeleteConfirm` pattern:

@ref src/tui/app.rs#StatusPicker@befc187590c51657354f9801df807d4b7d7fae4e

With a `new()` that defaults to `active: false, selected: 0, doc_path: PathBuf::new()`.

Add `pub status_picker: StatusPicker` to the `App` struct. Initialise it in `App::new()`.

Add three methods on `App`:

- `open_status_picker()` -- guard on `selected_doc_meta()` returning `Some`. Read the doc's current status, map it to an index (Draft=0, Review=1, Accepted=2, Rejected=3, Superseded=4), set `status_picker.selected` to that index, store the doc path, set `active = true`.
- `close_status_picker()` -- reset to defaults.
- `confirm_status_change(&mut self, root: &Path)` -- read `status_picker.selected`, map index back to a `Status` variant, call `cli::update::run(root, doc_path_str, &[("status", &status.to_string())])`, then `store.reload_file(root, &relative_path)`, then `rebuild_doc_tree(config)`, then `close_status_picker()`. Return `Result<()>`.

Also add `open_status_picker` for Filters mode: use `selected_filtered_doc()` instead of `selected_doc_meta()` to get the doc path when in Filters mode. The simplest approach is a single method that checks `self.view_mode` to pick the right accessor.

**How to verify:**
`cargo test` -- unit tests in Task 3 cover this.

---

### Task 2: Key handling and overlay dispatch

**ACs addressed:** AC1, AC2, AC4, AC5, AC6

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

In `handle_key()`, add a check for `self.status_picker.active` in the overlay priority chain, after `delete_confirm` and before `search_mode`:

```rust
if self.status_picker.active {
    return self.handle_status_picker_key(code, root);
}
```

Implement `handle_status_picker_key(&mut self, code: KeyCode, root: &Path)`:
- `KeyCode::Char('j') | KeyCode::Down` -- increment `selected`, clamp to 4 (five statuses).
- `KeyCode::Char('k') | KeyCode::Up` -- decrement `selected`, clamp to 0.
- `KeyCode::Enter` -- call `self.confirm_status_change(root)`.
- `KeyCode::Esc` -- call `self.close_status_picker()`.

In `handle_normal_key()`, add `KeyCode::Char('s')` for both Types mode (the default match arm) and Filters mode (the Filters match arm). Both call `self.open_status_picker()`.

**How to verify:**
`cargo test` -- unit tests in Task 3 cover this.

---

### Task 3: Tests

**ACs addressed:** AC1, AC2, AC3, AC4, AC5, AC6

**Files:**
- Create: `tests/tui_status_picker_test.rs`

**What to implement:**

Follow the `tui_delete_dialog_test.rs` pattern. Use `TestFixture` and `App::new`.

Planned tests:

1. **`test_open_status_picker_populates_from_selected_doc`** (AC1) -- Select an RFC with status "draft". Call `open_status_picker()`. Assert `status_picker.active == true`, `status_picker.selected == 0` (draft index), `status_picker.doc_path` matches the doc.

2. **`test_open_status_picker_preselects_current_status`** (AC1) -- Write an RFC with status "accepted". Call `open_status_picker()`. Assert `status_picker.selected == 2` (accepted index).

3. **`test_status_picker_navigation`** (AC2) -- Open picker on a draft doc (selected=0). Send `j` key. Assert selected == 1. Send `k` key. Assert selected == 0. Send `k` again. Assert selected == 0 (clamped).

4. **`test_confirm_status_change_updates_frontmatter`** (AC4) -- Open picker on a draft doc. Set `status_picker.selected = 2` (accepted). Call `confirm_status_change(root)`. Read the file from disk, assert frontmatter contains `status: accepted`. Assert `store.get()` returns the doc with `Status::Accepted`. Assert `status_picker.active == false`.

5. **`test_cancel_status_picker_no_changes`** (AC5) -- Open picker on a draft doc. Call `close_status_picker()`. Assert file still has `status: draft`. Assert `status_picker.active == false`.

6. **`test_status_picker_on_empty_list_noop`** (AC1) -- No docs. Call `open_status_picker()`. Assert `status_picker.active == false`.

7. **`test_status_picker_in_filters_mode`** (AC6) -- Switch to `ViewMode::Filters`. Select a doc. Call `open_status_picker()`. Assert picker opens correctly.

8. **`test_handle_key_s_opens_picker`** (AC1, AC6) -- In Types mode with a doc selected, send `s` key via `handle_key()`. Assert `status_picker.active == true`.

**How to verify:**
`cargo test tui_status_picker`

---

### Task 4: Overlay rendering

**ACs addressed:** AC1, AC3

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Add a `draw_status_picker(f: &mut Frame, app: &App)` function following the `draw_delete_confirm` pattern.

Render a centered popup (width ~25, height ~9). Use `Clear` widget, rounded borders, border colour cyan (to match selection highlighting).

List all five statuses as lines. Each status line is coloured using the existing `status_color()` function (Draft=Yellow, Review=Blue, Accepted=Green, Rejected=Red, Superseded=DarkGray). The line at index `app.status_picker.selected` gets a `> ` prefix and bold styling.

Add a help line at the bottom: `[j/k: select] [Enter: confirm] [Esc: cancel]` in DarkGray.

Call this function from the main `draw()` function, guarded by `if app.status_picker.active`, after the existing delete confirm draw call.

**How to verify:**
`cargo run` then press `s` on a document. Visual check: popup appears with coloured statuses, selection moves with j/k, Enter changes the status, Esc dismisses.

## Test Plan

| Test | AC | Properties traded |
|------|----|-------------------|
| open populates from selected doc | AC1 | -- |
| preselects current status | AC1 | -- |
| j/k navigation with clamping | AC2 | -- |
| confirm writes frontmatter + reloads store | AC4 | Trades Fast slightly (disk I/O via TestFixture) for Predictive |
| cancel preserves file | AC5 | -- |
| empty list is noop | AC1 | -- |
| works in Filters mode | AC6 | -- |
| handle_key 's' opens picker | AC1, AC6 | -- |

AC3 (status colours) is covered visually. The colour mapping is already tested implicitly via `status_color()` being a pure function with no branching complexity worth unit testing in isolation. The rendering test would require a frame buffer assertion, which trades Writable for marginal confidence.

## Notes

The `cli::update::run` function takes a `&str` doc path relative to root. The `StatusPicker` stores a `PathBuf`. The confirm method needs to convert via `doc_path.to_str()` or strip the root prefix, depending on whether the stored path is relative or absolute. Match the pattern used by `confirm_delete` which stores relative paths.
