---
title: TUI Startup Performance
type: audit
status: draft
author: jkaloger
date: 2026-03-19
tags: []
related:
- related-to: docs/rfcs/RFC-001-my-first-rfc.md
---


## Scope

Performance audit of the TUI startup path. User-reported symptoms: perceptible lag before the UI becomes interactive, and swallowed keyboard inputs (e.g. `hjkl` navigation keys ignored) during the first moments after launch. Observed on both small and large lazyspec codebases.

## Criteria

1. Time-to-interactive: the TUI should accept and process keypresses within one frame (~16ms) of the alternate screen appearing
2. No input loss: every keypress after `enable_raw_mode()` should be captured and processed
3. No unnecessary synchronous work before the first frame render

## Findings

### Finding 1: stdin contention between probe thread and input thread

**Severity:** critical
**Location:** `src/tui/mod.rs:117-122` (probe thread), `src/tui/mod.rs:144-157` (input thread), `src/tui/terminal_caps.rs:30`
**Description:** The probe thread calls `Picker::from_query_stdio()` which sends escape sequences to stdout and reads responses from stdin. The input thread calls `crossterm::event::read()` which also performs a blocking read on stdin. Both threads start nearly simultaneously and share the same file descriptor with no synchronization. During the `from_query_stdio()` window, user keypresses can be consumed by the probe thread and discarded, or terminal capability response bytes can be consumed by crossterm and interpreted as garbage key events. This is the most likely cause of the reported swallowed inputs.

Note: the subprocess spawns for `d2 --version` and `mmdc --version` (`ToolAvailability::detect()`) run sequentially after `from_query_stdio()` completes in the same thread, so they do not extend the stdin contention window. They do delay the `ProbeResult` message, but that is a separate concern (see Finding 4).

**Recommendation:** Sequence the probe and input threads so they never read stdin concurrently. Either complete the terminal capability probe before spawning the input thread, or use a mechanism that doesn't read from stdin (e.g. check `$TERM_PROGRAM` environment variables instead of querying the terminal).

### Finding 2: synchronous Store::load blocks before TUI enters alternate screen

**Severity:** high
**Location:** `src/main.rs:121`, `src/engine/store.rs:34-90`
**Description:** `Store::load()` reads every `.md` file (full file contents, not just frontmatter) under every configured type directory using sequential `fs::read_to_string()` calls, including nested child documents (`store.rs:84`). This all happens on the main thread before `enable_raw_mode()` (mod.rs:102) or `EnterAlternateScreen` (mod.rs:104).

During this window the user sees their normal terminal, not a blank screen, but there is no feedback that loading is in progress. On a large codebase with many documents, this is the primary source of perceived startup lag: the user runs `lazyspec`, nothing visibly happens, and then the TUI snaps into view.

**Recommendation:** Either defer full file reads (load frontmatter only via a streaming parser that stops after the closing `---`, read bodies lazily on selection), parallelize file reads with rayon, or enter the alternate screen and render a loading indicator before `Store::load()` begins.

### Finding 3: synchronous validation before first frame

**Severity:** medium
**Location:** `src/tui/mod.rs:110`, `src/tui/app.rs:433-439`
**Description:** `app.refresh_validation(config)` runs `validate_full()` over all loaded documents (link checks, rule checks, duplicate ID scan) after `App::new()` but before the event loop starts. This adds to the time before the first frame is rendered. The actual cost of `validate_full()` has not been profiled; on small codebases it may be negligible, but it scales with document count and link density.

`refresh_validation()` also calls `rebuild_search_index()`, which was already called at the end of `App::new()` (see Finding 5). This is a redundant call but its cost contribution is addressed there, not here.

**Recommendation:** Defer validation to after the first frame render, or run it in a background thread and update the UI when results arrive. Profile `validate_full()` to determine actual cost before investing in more complex solutions.

### Finding 4: probe thread delays ProbeResult with synchronous subprocess spawns

**Severity:** low
**Location:** `src/tui/diagram.rs:33-39`, `src/tui/diagram.rs:120-125`, `src/tui/mod.rs:117-122`
**Description:** `ToolAvailability::detect()` spawns `d2 --version` and `mmdc --version` as blocking subprocesses in the same thread as the terminal capability probe. These subprocesses do not read stdin, so they do not contribute to Finding 1's stdin contention. However, they delay the `ProbeResult` event, meaning the app runs with the fallback `halfblocks` picker and unknown tool availability for longer than necessary. If PATH resolution is slow (e.g. NFS home directory, nix shell wrapper), these can add hundreds of milliseconds.

**Recommendation:** Decouple tool availability detection from the terminal capability probe. Run them in separate threads, or move tool detection to first-use rather than startup.

### Finding 5: duplicate search index build on startup

**Severity:** low
**Location:** `src/tui/app.rs:428` (`App::new`), `src/tui/app.rs:438` (`refresh_validation`)
**Description:** `rebuild_search_index()` is called once at the end of `App::new()` and then again inside `refresh_validation()` which is called immediately after on `mod.rs:110`. The second call is redundant since no documents change between the two calls. This is a one-line fix and the easiest finding to address.

**Recommendation:** Remove the `rebuild_search_index()` call from `App::new`, since `refresh_validation()` will always be called immediately after during init and will rebuild it.

### Finding 6: spawned threads are fire-and-forget with no cleanup

**Severity:** low
**Location:** `src/tui/mod.rs:117` (probe thread), `src/tui/mod.rs:144` (input thread), `src/tui/mod.rs:263-272` (exit path)
**Description:** Neither `std::thread::spawn` call stores its `JoinHandle`. The exit path (`mod.rs:263-272`) breaks from the loop, disables raw mode, leaves the alternate screen, and returns. No threads are joined. If the user quits quickly (e.g. presses `q` before `ProbeResult` arrives), the probe thread may still be running subprocess calls or reading stdin. The input thread loops forever on `crossterm::event::read()` with no shutdown signal (the `input_paused` flag only causes a sleep, never an exit). In practice the OS cleans up on process exit, but this prevents orderly shutdown and could cause issues if `run()` is ever called in a context where the process continues.

**Recommendation:** Store `JoinHandle`s. Add an `AtomicBool` shutdown flag checked by both threads. Join threads before returning from `run()`.

### Finding 7: unspecified timing between thread spawns and first render

**Severity:** info
**Location:** `src/tui/mod.rs:117-164`
**Description:** The probe thread (line 117), filesystem watcher (line 126), and input thread (line 144) are all spawned before the first `terminal.draw()` (line 164). There is no synchronization to ensure the input thread is actively reading before the first frame renders. In practice on modern hardware the thread will be scheduled quickly, but the ordering is not guaranteed. If the first frame renders and the user presses a key before the input thread's first `crossterm::event::read()` call, that keypress could be buffered by the OS and delivered on the next read, or in theory lost if the terminal's input buffer is flushed. This is unlikely to cause issues in practice but is a correctness gap.

**Recommendation:** Use a barrier or oneshot channel to confirm the input thread is ready before entering the main loop.

## Summary

The startup path has two distinct problems that map to the two reported symptoms.

Swallowed inputs are caused by Finding 1: the probe thread and input thread both read from stdin concurrently during the `from_query_stdio()` window, creating a race where keypresses are consumed and discarded by the wrong reader. This affects all codebases regardless of size. Findings 6 and 7 are related correctness gaps in thread lifecycle management.

Startup lag is caused by Findings 2 and 3: synchronous document loading (full file reads, not just frontmatter) and validation block the main thread before the alternate screen appears. The user sees their normal terminal with no loading feedback. Severity scales with document count. Finding 5 (duplicate search index build) is the lowest-effort fix in this group.

The quickest wins in order of effort: Finding 5 (one-line delete), Finding 1 (sequence probe before input thread), Finding 6 (store handles and add shutdown flag). Findings 2 and 3 require architectural changes to defer work or introduce async loading.
