---
title: Fix stdin contention and thread lifecycle
type: iteration
status: accepted
author: jkaloger
date: 2026-03-19
tags: []
related:
- implements: docs/stories/STORY-068-reliable-input-handling-on-tui-startup.md
---



## Task Breakdown

### Task 1: Sequence probe before input thread (F1)

**ACs addressed:** AC1, AC2

**Files:**
- M `src/tui/mod.rs` (lines 105-157)
- M `src/tui/terminal_caps.rs`

Move `Picker::from_query_stdio()` out of the background thread and run it synchronously between `enable_raw_mode()` and the input thread spawn. The probe needs raw mode active and must complete before anything else reads stdin. The result replaces the `halfblocks()` fallback assigned at line 105.

```
enable_raw_mode()
EnterAlternateScreen
picker = create_picker()          // synchronous, stdin is uncontested
App::new(store, config, picker)   // now uses real picker from the start
// spawn tool availability thread (no stdin reads)
// spawn input thread (sole stdin reader from here on)
```

`create_picker()` already falls back to `halfblocks()` on error, so this doesn't change failure behavior. The `ProbeResult` event type can be split: `PickerResult` is no longer needed since the picker is known at init. `ToolAvailabilityResult` becomes the only async probe event.

### Task 2: Decouple tool availability detection from probe thread (F4)

**ACs addressed:** AC2

**Files:**
- M `src/tui/mod.rs` (lines 117-122)
- M `src/tui/diagram.rs`

After Task 1 removes the picker probe from the background thread, `ToolAvailability::detect()` becomes the sole purpose of the probe thread. Rename it for clarity. This is mostly a cleanup that falls out of Task 1, but ensures the subprocess spawns (`d2 --version`, `mmdc --version`) are clearly separated from any stdin-touching code.

### Task 3: Add shutdown signal and join threads on exit (F6)

**ACs addressed:** AC3

**Files:**
- M `src/tui/mod.rs` (lines 117, 144, 263-272)

Store `JoinHandle`s for both spawned threads. Add a `shutdown: Arc<AtomicBool>` checked in the input thread's loop (alongside the existing `input_paused` check). On the exit path (line 263), set the shutdown flag and join both handles.

The input thread blocks on `crossterm::event::read()` which won't wake on the flag alone. Options:
- Use `crossterm::event::poll(Duration::from_millis(50))` before `read()` so the thread checks shutdown periodically
- Or accept that the input thread may block for one final read after shutdown is signaled (the OS will clean it up on process exit)

The pragmatic choice is `poll` with a short timeout, since it also lets us remove the `input_paused` sleep hack.

### Task 4: Input thread readiness barrier (F7)

**ACs addressed:** AC4

**Files:**
- M `src/tui/mod.rs` (lines 144-160)

Add a `std::sync::Barrier::new(2)` (or a oneshot channel). The input thread signals readiness after entering its loop but before the first `read()`/`poll()`. The main thread waits on the barrier before entering the event loop. This guarantees the input thread is consuming stdin before the first frame renders.

## Test Plan

- Launch the TUI and immediately press `j` repeatedly. All keypresses should register. Repeat 10+ times to exercise the race window that previously existed.
- Launch the TUI and press `q` within 100ms. The process should exit cleanly without hanging threads.
- Verify `LAZYSPEC_PERF_LOG=1` shows no `ProbeResult` event in the channel (picker is now synchronous). A `ToolAvailabilityResult` event should appear after startup.
- On a terminal without Kitty/Sixel support, confirm the TUI falls back to halfblocks without error.

## Notes

Task 1 is the critical fix. Tasks 2-4 are correctness improvements that reduce the surface for future regressions. Task 3's `poll` approach has a minor tradeoff: it introduces a 50ms worst-case latency on the final keypress before shutdown, which is acceptable.
