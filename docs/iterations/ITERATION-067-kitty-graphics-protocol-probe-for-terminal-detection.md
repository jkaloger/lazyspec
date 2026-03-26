---
title: Kitty graphics protocol probe for terminal detection
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: STORY-063
---




## Context

ITERATION-065/066 used env var sniffing (`TERM_PROGRAM`, `TERM`) to detect terminal image protocol support. This breaks inside multiplexers like tmux, which override these vars. Replace with a direct kitty graphics protocol probe.

**ACs addressed:** AC1 (replaces existing implementation)

## Changes

### Task 1: Replace env var detection with kitty graphics probe

**ACs addressed:** AC1

**Files:**
- Rewrite: `src/tui/terminal_caps.rs`
- Modify: `src/tui/mod.rs` (probe between raw mode and alternate screen, pass result to App)
- Modify: `src/tui/app.rs` (accept protocol as parameter instead of detecting internally)
- Modify: `tests/tui_diagram_test.rs` (remove env var tests)

**What to implement:**

Replace `detect()` and `detect_from()` in `terminal_caps.rs` with a single `probe()` function.

**Probe sequence:**
1. Write the APC query to stdout: `\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\`
2. Read raw bytes from stdin with a 100ms timeout
3. If the response contains `OK` -> return `KittyGraphics`
4. If timeout or no `OK` -> return `Unsupported`

Delete `detect_from()` entirely. No env var logic.

> [!WARNING]
> The probe must run after `enable_raw_mode()` (so we can read terminal responses) but before `EnterAlternateScreen` (so we're not fighting the alternate buffer). Call `probe()` in `tui::run()` between these two steps, and pass the result to `App::new()`.

Change `App::new()` signature to accept `protocol: TerminalImageProtocol` as a parameter instead of calling `detect()` internally.

For terminal response parsing: the terminal sends back `\x1b_Gi=31;OK\x1b\\`. APC responses aren't crossterm events, so read raw bytes from stdin directly. Use `std::io::Read` on stdin in a loop, with `crossterm::event::poll()` for timeout. Accumulate into a buffer and check for `OK`. After the probe completes (success or timeout), drain remaining bytes to avoid polluting the event loop.

**How to verify:**
```
cargo test
cargo run  # visually confirm in Ghostty+tmux (needs allow-passthrough on)
```

## Test Plan

The `probe()` function depends on a real terminal and cannot be unit tested deterministically. Remove all `detect_from` tests.

Remove: `test_detect_kitty_protocol`, `test_detect_kitty_via_term`, `test_detect_iterm_protocol`, `test_detect_wezterm_protocol`, `test_detect_ghostty_protocol`, `test_detect_none_protocol`, `test_detect_none_when_no_env`.

Keep: all hint tests, tool availability tests, extraction tests -- these pass protocol values explicitly and don't depend on detection.

## Notes

- Requires `allow-passthrough on` in tmux for the probe to reach the outer terminal.
- `Sixel` variant stays in the enum for forward compatibility but nothing returns it currently. DA1 probe can be added later.
