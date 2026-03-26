---
title: Open in Editor
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: STORY-017
---




## Changes

### Task 1: Editor resolution helper

**ACs addressed:** AC2

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a function `resolve_editor() -> String` that returns the editor command by checking environment variables in order:

1. `$EDITOR` if set and non-empty
2. `$VISUAL` if set and non-empty
3. `"vi"` as the final fallback

This is a standalone function (not a method on `App`) since it has no state dependency.

**How to verify:**
Unit test with overridden env vars. See test plan.

---

### Task 2: Terminal suspend/resume and editor spawn

**ACs addressed:** AC1, AC3

**Files:**
- Modify: `src/tui/mod.rs`
- Modify: `src/tui/app.rs`

**What to implement:**

Add a field `pub editor_request: Option<PathBuf>` to `App`. When the user presses `e` and a document is selected, set `editor_request` to `Some(full_path)` instead of directly launching the editor (since `App` doesn't own the terminal).

In the main loop in `src/tui/mod.rs`, after `app.handle_key(...)`, check `app.editor_request.take()`. If `Some(path)`:

1. `execute!(terminal.backend_mut(), LeaveAlternateScreen)`
2. `disable_raw_mode()`
3. Spawn `Command::new(resolve_editor()).arg(&path).status()` -- this blocks until the editor exits
4. `enable_raw_mode()`
5. `execute!(terminal.backend_mut(), EnterAlternateScreen)`
6. `terminal.clear()` to force a full redraw

The suspend/resume logic should be in a helper function `run_editor(terminal, path) -> Result<()>` in `src/tui/mod.rs` so it can be reused by future "shell out" features.

**How to verify:**
Manual test: run `EDITOR=vim cargo run`, press `e` on a document, verify vim opens, edit and quit, verify TUI resumes cleanly.

---

### Task 3: Document reload after editing

**ACs addressed:** AC4

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

After the editor exits and terminal is resumed (in the main loop, after `run_editor` returns), reload the edited document:

```rust
if let Ok(relative) = path.strip_prefix(&root) {
    let _ = app.store.reload_file(&root, relative);
}
```

This reuses the same `reload_file` pattern already used by the file watcher at `mod.rs:58`. The file watcher will also catch the change independently, but an explicit reload here ensures the display updates immediately rather than waiting for the next watcher poll.

**How to verify:**
Manual test: open a document with `e`, change the title in frontmatter, save and quit, verify the TUI shows the updated title immediately.

---

### Task 4: Wire `e` key in Types/Filters mode and Graph mode

**ACs addressed:** AC1, AC5

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

In `handle_normal_key`, add the `e` binding in two places:

1. In the Graph mode match block (inside `if self.view_mode == ViewMode::Graph`), add:
   ```rust
   KeyCode::Char('e') => {
       if let Some(node) = self.graph_nodes.get(self.graph_selected) {
           self.editor_request = Some(root.join(&node.path));
       }
   }
   ```
   This requires `handle_normal_key` to receive `root: &Path` (it currently doesn't). Update the signature and the call site in `handle_key`.

2. In the Types/Filters match block (the `match (code, modifiers)` block), add:
   ```rust
   (KeyCode::Char('e'), _) => {
       if let Some(doc) = self.selected_doc_meta() {
           self.editor_request = Some(root.join(&doc.path));
       }
   }
   ```

The `e` key should be ignored when no document is selected (the `if let Some(...)` handles this).

**How to verify:**
Unit tests asserting `editor_request` is set correctly. See test plan.

## Test Plan

### Test 1: `resolve_editor` returns `$EDITOR` when set
Set `EDITOR=nano`, verify `resolve_editor()` returns `"nano"`.
**Properties:** Isolated, deterministic, fast, specific.

### Test 2: `resolve_editor` falls back to `$VISUAL`
Unset `EDITOR`, set `VISUAL=code`, verify `resolve_editor()` returns `"code"`.
**Properties:** Isolated, deterministic, fast, specific.
**Tradeoff:** These tests mutate env vars which affects process-global state. Use `std::env::set_var`/`remove_var` and run with `--test-threads=1` or use a serial test approach. Alternatively, refactor `resolve_editor` to accept env values as parameters for pure testability.

### Test 3: `resolve_editor` falls back to `vi`
Unset both `EDITOR` and `VISUAL`, verify `resolve_editor()` returns `"vi"`.
**Properties:** Isolated, deterministic, fast, specific.

### Test 4: `e` key sets `editor_request` in Types mode
Create an `App` with documents, press `e`, verify `app.editor_request` is `Some` with the correct path.
**Properties:** Isolated, behavioral, fast, specific.

### Test 5: `e` key sets `editor_request` in Graph mode
Create an `App` with graph nodes, switch to Graph mode, press `e`, verify `app.editor_request` is `Some` with the correct path.
**Properties:** Isolated, behavioral, fast, specific.

### Test 6: `e` key is no-op when no document selected
Create an `App` with no documents, press `e`, verify `app.editor_request` is `None`.
**Properties:** Isolated, behavioral, fast, specific.

### Test 7: `e` key ignored during modal states
Open create form, press `e`, verify `app.editor_request` is `None`. Same for delete confirm, search, help overlay.
**Properties:** Isolated, behavioral, fast, structure-insensitive.

> [!NOTE]
> Tests 1-3 (env var tests) trade Isolated for Predictive -- they test the actual env lookup rather than a mock. To keep them isolated from each other, accept the `resolve_editor` function taking explicit values or run them serially.

## Notes

The `handle_normal_key` signature change (adding `root: &Path`) is the only structural change. It's needed because `editor_request` stores the full path, which requires joining with root. The `root` parameter is already available in `handle_key` and just needs to be threaded through.

The `editor_request` field pattern (set in `App`, consumed in the main loop) follows the same principle as `should_quit` -- the `App` signals intent, the loop acts on it. This keeps terminal ownership in `mod.rs` where it belongs.
