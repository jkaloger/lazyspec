---
title: TUI input lag audit
type: audit
status: draft
author: jkaloger
date: 2026-03-16
tags: []
related:
- related-to: docs/architecture/ARCH-005-tui/threading-model.md
- related-to: docs/architecture/ARCH-005-tui/index.md
---



## Scope

Performance audit of TUI keystroke handling. User reports keystrokes being "eaten" during normal usage. Audit covers the full input-to-render pipeline: event polling, main loop timing, synchronous work on the hot path, and per-keystroke operations.

## Criteria

1. Keystrokes must not be dropped under normal usage
2. Input-to-display latency should be under 50ms for typed characters
3. The main loop must not perform blocking I/O on every iteration
4. Per-keystroke handlers must not do O(N) work over the full document set
5. The render path must not duplicate work already done in the event loop

## Findings

### Finding 1: Main loop blocks on 100ms recv_timeout before processing input

**Severity:** high
**Location:** `src/tui/mod.rs:173`
**Description:** The main loop calls `rx.recv_timeout(Duration::from_millis(100))`. Combined with the input thread's 50ms poll timeout (`mod.rs:148`), worst-case latency from keypress to render is 150ms+ before any processing begins. This is the primary source of perceived lag across all modes.
**Recommendation:** Reduce the recv timeout, or restructure so rendering is event-driven rather than polling-driven. A common pattern is to use a separate render tick channel with a shorter interval, or to wake the main loop immediately when a terminal event arrives (the unbounded channel already does this, but the 100ms timeout means the *previous* iteration's render blocks for that long before the next recv).

### Finding 2: Synchronous disk I/O in the draw closure

**Severity:** high
**Location:** `src/tui/ui.rs:458`, `src/tui/ui.rs:791`
**Description:** Both `draw_preview_content` and `draw_fullscreen` call `app.store.get_body_raw()` when the expansion cache is cold. `get_body_raw` does a synchronous `fs::read_to_string` (`store.rs:270`). This blocks the main thread during rendering, which is the most timing-sensitive part of the loop. Every navigation to a new document triggers this.
**Recommendation:** Never do disk I/O inside the draw closure. Use a placeholder ("Loading...") when the cache is cold and let `request_expansion` populate the cache asynchronously. The draw path should only read from in-memory caches.

### Finding 3: Synchronous disk I/O in request_expansion on every loop tick

**Severity:** medium
**Location:** `src/tui/app.rs:794`
**Description:** `request_expansion` calls `store.get_body_raw()` (disk read) on the main thread every loop iteration until the expansion cache is warm for the current document. While the expansion itself is spawned to a thread, the initial file read that feeds it is synchronous.
**Recommendation:** Move the `get_body_raw` call into the spawned thread. Pass just the file path and let the background thread handle the read.

### Finding 4: update_search does O(N) work on every keystroke

**Severity:** medium
**Location:** `src/tui/app.rs:922-945`
**Description:** Every character typed in search mode calls `store.all_docs()`, then runs `.to_lowercase()` on every document's title, tags, and path. This allocates new strings for every document on every keystroke. For a project with hundreds of documents, this adds measurable latency per character.
**Recommendation:** Debounce search updates (e.g. 100ms after last keystroke), or pre-compute a lowercase search index that is invalidated on store changes rather than rebuilt per keystroke.

### Finding 5: extract_diagram_blocks called every loop iteration unconditionally

**Severity:** medium
**Location:** `src/tui/mod.rs:164`
**Description:** The main loop calls `extract_diagram_blocks(&body)` every iteration for the currently selected document, even when the body hasn't changed. This does a linear scan with regex matching and allocations on every tick.
**Recommendation:** Cache the extracted blocks keyed on the body content hash. Only re-extract when the body changes.

### Finding 6: Double extraction of diagram blocks during render

**Severity:** low
**Location:** `src/tui/diagram.rs:299` (via `inject_fallback_hints`)
**Description:** `build_preview_segments` calls `extract_diagram_blocks` once, then `inject_fallback_hints` calls it a second time on the same body string. The body is parsed twice per draw frame when non-renderable diagram blocks are present.
**Recommendation:** Pass the already-extracted blocks into `inject_fallback_hints` instead of re-extracting.

### Finding 7: filtered_docs and all_docs called redundantly every draw frame

**Severity:** low
**Location:** `src/tui/ui.rs:1394-1396`
**Description:** In Filters mode, the draw function calls both `app.filtered_docs()` (which sorts) and `app.store.all_docs()` on every frame. `filtered_docs` re-sorts its results every time it's called.
**Recommendation:** Cache `filtered_docs` results and invalidate on store/filter changes. Use `all_docs().len()` or a stored count rather than building the full list just for counting.

### Finding 8: Input thread poll timeout adds baseline latency

**Severity:** info
**Location:** `src/tui/mod.rs:148`
**Description:** The input thread uses `crossterm::event::poll(Duration::from_millis(50))`. This means there's up to 50ms between a keypress occurring and the event being read. While this is a reasonable default, it compounds with Finding 1.
**Recommendation:** Consider reducing to 10-20ms, or using a blocking `read()` call (the thread is dedicated to input, so blocking is fine). A blocking read would eliminate this latency entirely.

## Summary

The main sources of keystroke lag are architectural rather than algorithmic:

The **100ms recv_timeout** (Finding 1) and **50ms input poll** (Finding 8) together create a baseline 150ms worst-case latency floor before any processing happens. This alone explains the "eaten keystrokes" feel.

**Synchronous disk I/O in the draw closure** (Finding 2) is the most impactful single issue. When navigating between documents, the render path blocks on `fs::read_to_string`, stalling the entire main loop.

The per-keystroke search scan (Finding 4) and unconditional diagram extraction (Finding 5) add unnecessary work that compounds with the timing issues above.

Fixing Findings 1, 2, and 8 would have the most immediate impact on perceived responsiveness. The remaining findings are optimisations that would improve behaviour under load.
