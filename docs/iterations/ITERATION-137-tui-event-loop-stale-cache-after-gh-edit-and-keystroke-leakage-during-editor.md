---
title: "TUI event loop: stale cache after GH edit and keystroke leakage during editor"
type: iteration
status: accepted
author: "agent"
date: 2026-03-28
tags: []
related: []
---


## Changes

### Task 1: Invalidate caches after editor returns

**Bug:** After editing a GitHub-issues doc from the TUI, the display shows stale content because `expanded_body_cache` and `disk_cache` retain the pre-edit body.

**Files:**
- Modify: `src/tui/infra/event_loop.rs`

**What to implement:**

In the editor request handler (around line 384), after `app.store.reload_file()`, add:

```rust
app.expanded_body_cache.remove(relative);
app.disk_cache.invalidate(relative);
```

This matches the invalidation pattern in the `FileChange` handler (lines 91-93).

**How to verify:**
1. `cargo build`
2. Manual test: open TUI, edit a GitHub-issues doc, confirm the TUI displays the updated content after the editor closes.

### Task 2: Invalidate caches in GhPushResult handler

**Bug:** Same stale display issue. When the background GitHub push completes and sends `GhPushResult`, the handler reloads the store but does not invalidate `expanded_body_cache` or `disk_cache`. If the GitHub response differs from the local edit (normalization, whitespace), the display stays stale.

**Files:**
- Modify: `src/tui/infra/event_loop.rs`

**What to implement:**

In the `AppEvent::GhPushResult(Ok(()))` handler (around line 140-146), after reloading the store, clear both caches:

```rust
app.expanded_body_cache.clear();
app.disk_cache.clear();
```

Using `clear()` rather than targeted `remove()` because the GhPushResult handler doesn't track which specific path was edited (unlike the editor handler which has `relative` in scope). This matches the `has_non_md` branch of the FileChange handler (lines 100-101).

**How to verify:**
1. `cargo build`
2. Manual test: edit a GitHub-issues doc, wait for the push to complete, confirm TUI shows up-to-date content.

### Task 3: Replace blocking read() with poll() in input thread

**Bug:** The input thread calls blocking `crossterm::event::read()` at line 274. If the thread enters `read()` before `paused` is set to `true`, it sits on stdin competing with the editor process. Keystrokes intended for the editor get captured by crossterm and queued as `AppEvent::Terminal` events.

**Files:**
- Modify: `src/tui/infra/event_loop.rs`

**What to implement:**

Replace the input thread body (lines 267-280) with a poll-based loop:

```rust
std::thread::spawn(move || {
    loop {
        if paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        // Poll with short timeout so we re-check paused frequently
        match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = crossterm::event::read() {
                    perf_log::log(&format!("input_thread: read key {:?}", key.code));
                    let _ = term_tx.send(AppEvent::Terminal(key));
                    perf_log::log("input_thread: sent to channel");
                }
            }
            _ => {}
        }
    }
});
```

`poll(50ms)` returns immediately if an event is available, or waits up to 50ms. This means the thread never blocks for longer than 50ms, so it will observe the `paused` flag promptly and stop reading from stdin before the editor is launched.

**How to verify:**
1. `cargo build`
2. Manual test: open TUI, edit a doc, type several characters in the editor, exit editor. Confirm the TUI is on the same screen as before (not search mode or elsewhere), and no phantom keystrokes were processed.

## Test Plan

Both bugs are in the terminal event loop, which spawns threads, launches external processes, and requires a real terminal. Automated unit tests are not practical here without substantial test infrastructure (mocking crossterm, terminal, and stdin). The appropriate verification is:

1. **Stale cache (Tasks 1-2):** Edit a GitHub-issues doc from TUI. Confirm TUI displays updated content immediately after editor closes and again after GhPush completes. Repeat 3 times.
2. **Keystroke leakage (Task 3):** Edit a doc from TUI. In the editor, type substantial text and use navigation keys. Exit editor. Confirm TUI state is unchanged (same view, no search mode, no phantom navigation). Repeat 3 times.
3. **Regression:** Navigate normally in TUI, confirm key responsiveness is unchanged (poll-based loop should have no perceptible latency impact since poll returns immediately when events are available).

Tradeoff: these are manual tests. They sacrifice Fast and Deterministic for Predictive and Inspiring (they test the actual terminal behavior). Automated tests for this code would require a terminal emulator harness that doesn't exist in this project.

## Notes

- The `CreateComplete` handler (line 159-185) has the same missing cache invalidation pattern as `GhPushResult`. It doesn't affect this bug (create form has its own flow), but is a latent issue to address separately.
- `Ordering::Relaxed` on the `paused` flag is sufficient here. We don't need happens-before guarantees; we just need the flag to propagate within a bounded time, which the 50ms poll timeout ensures.
