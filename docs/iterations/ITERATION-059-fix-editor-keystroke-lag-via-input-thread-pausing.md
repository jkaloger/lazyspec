---
title: Fix editor keystroke lag via input thread pausing
type: iteration
status: accepted
author: agent
date: 2026-03-12
tags: []
related:
- implements: docs/stories/STORY-017-open-in-editor.md
---



## Problem

The dedicated terminal input thread (`src/tui/mod.rs:113-121`) continuously
polls `crossterm::event::read()` in an infinite loop. When `run_editor` spawns
nvim (or any `$EDITOR`), this thread keeps running and races with the child
process for stdin. Keystrokes intended for the editor get intercepted and
buffered into the channel, causing lag on editor open and lost input.

The same race affects the `resume_request` block (agent resume via `claude
--resume`).

## Approach

Add a shared `Arc<AtomicBool>` (`input_paused`) between the main loop and the
input thread. The input thread checks the flag each poll cycle and sleeps
instead of reading when paused. Before any subprocess that needs stdin
(editor, agent resume), set the flag to `true`. After the subprocess exits,
drain stale `Terminal` events from the channel, then set the flag back to
`false`.

## Changes

### Task 1: Add AtomicBool pause flag to the input thread

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

1. Add `use std::sync::atomic::{AtomicBool, Ordering};` and `use std::sync::Arc;` to imports.
2. Before the input thread spawn (line 112), create the flag:
   ```rust
   let input_paused = Arc::new(AtomicBool::new(false));
   ```
3. Clone it into the thread closure:
   ```rust
   let paused = input_paused.clone();
   ```
4. Inside the thread loop, check the flag before polling. When paused, sleep
   for the poll interval and `continue` without calling `poll`/`read`:
   ```rust
   loop {
       if paused.load(Ordering::Relaxed) {
           std::thread::sleep(Duration::from_millis(50));
           continue;
       }
       if crossterm::event::poll(Duration::from_millis(50)).unwrap_or(false) {
           if let Ok(Event::Key(key)) = crossterm::event::read() {
               let _ = term_tx.send(AppEvent::Terminal(key));
           }
       }
   }
   ```

**How to verify:**
- `cargo test` passes (no behavioural change yet, flag defaults to false).
- `cargo clippy` clean.

### Task 2: Pause input around editor and agent subprocess launches

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

1. Before calling `run_editor` (around line 141), set the flag and drain:
   ```rust
   if let Some(path) = app.editor_request.take() {
       input_paused.store(true, Ordering::Relaxed);
       // drain any terminal events already buffered
       while let Ok(AppEvent::Terminal(_)) = rx.try_recv() {}
       run_editor(&mut terminal, &path)?;
       // drain events the thread may have read before pausing took effect
       while let Ok(AppEvent::Terminal(_)) = rx.try_recv() {}
       input_paused.store(false, Ordering::Relaxed);
       // ... existing reload logic
   }
   ```

   Note: the drain uses a pattern match on `AppEvent::Terminal(_)` only, so
   file-change and expansion events are not lost. Since `try_recv` returns
   the event by value and we only want to discard `Terminal` variants, we need
   a small helper or a loop that re-sends non-Terminal events. Simpler
   alternative: drain all events, since file watcher will re-fire and
   expansions will be re-requested. Choose the simpler drain-all approach
   unless testing reveals lost expansion results.

   Revised (drain all):
   ```rust
   if let Some(path) = app.editor_request.take() {
       input_paused.store(true, Ordering::Relaxed);
       while rx.try_recv().is_ok() {}
       run_editor(&mut terminal, &path)?;
       while rx.try_recv().is_ok() {}
       input_paused.store(false, Ordering::Relaxed);
       let root = app.store.root().to_path_buf();
       if let Ok(relative) = path.strip_prefix(&root) {
           let _ = app.store.reload_file(&root, relative);
       }
   }
   ```

2. Apply the same pattern to the `resume_request` block (line 148-163):
   ```rust
   #[cfg(feature = "agent")]
   if let Some(session_id) = app.resume_request.take() {
       input_paused.store(true, Ordering::Relaxed);
       while rx.try_recv().is_ok() {}

       execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
       disable_raw_mode()?;
       let _ = Command::new("claude")
           .args(["--resume", &session_id])
           .status();
       enable_raw_mode()?;
       execute!(terminal.backend_mut(), EnterAlternateScreen)?;
       terminal.clear()?;

       while rx.try_recv().is_ok() {}
       input_paused.store(false, Ordering::Relaxed);
       let root = app.store.root().to_path_buf();
       app.store = Store::load(&root, config)?;
   }
   ```

**How to verify:**
- `cargo build` succeeds.
- Manual test: `cargo run -- tui`, press `e` on a doc, type immediately in
  nvim. Keystrokes should arrive without lag or loss.
- Manual test: quit nvim, verify TUI redraws and responds to input normally.

## Test Plan

This bug is inherently about subprocess stdin races, which are difficult to
test deterministically in unit tests. The verification is manual:

1. **Editor launch responsiveness:** Open the TUI, press `e`, and immediately
   start typing in nvim. All keystrokes should register. Previously the first
   few would be swallowed.

2. **Editor return:** After quitting nvim, the TUI should redraw immediately
   and respond to keypresses without delay.

3. **No regression on normal TUI input:** Navigate the TUI (j/k, enter, tab,
   q) and verify responsiveness is unchanged.

4. **Agent resume (if feature enabled):** If the `agent` feature is available,
   test that `claude --resume` also receives input cleanly.

> Tradeoff: These are manual tests, trading Deterministic and Fast for
> Predictive. The race condition is timing-dependent and involves child process
> stdin inheritance, which cannot be reliably exercised in a unit test harness.

## Notes

- `Ordering::Relaxed` is sufficient here. We don't need happens-before
  guarantees across threads; we just need the flag to propagate within a few
  milliseconds, which relaxed ordering does on all modern architectures. The
  50ms sleep in the input thread provides a natural synchronisation window.
- The drain-all approach for the channel is simpler than selectively keeping
  non-Terminal events. File watcher events will re-fire naturally, and
  expansion results will be re-requested on the next loop iteration.
