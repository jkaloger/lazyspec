---
title: "TUI Lifecycle and Threading"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, threading, lifecycle]
related:
  - implements: docs/architecture/ARCH-005-tui/threading-model.md
  - implements: docs/architecture/ARCH-005-tui/event-loop.md
---

## Acceptance Criteria

### AC: raw-mode-and-alternate-screen-on-startup

Given the user launches the TUI via `lazyspec` with no subcommand
When the `run` function executes
Then raw mode is enabled and the alternate screen is entered before any drawing occurs

### AC: initial-halfblocks-picker

Given the TUI is starting up
When `App::new` is called before the probe thread completes
Then the app uses a `Picker::halfblocks()` as the initial image picker, avoiding blocking terminal queries on the main thread

### AC: unified-crossbeam-channel

Given the TUI has initialized
When threads need to communicate with the main loop
Then all events flow through a single unbounded `crossbeam_channel` as `AppEvent` variants

### AC: input-thread-forwards-keys

Given the input thread is running and `input_paused` is `false`
When the user presses a key
Then the input thread sends an `AppEvent::Terminal` containing the `KeyEvent` through the channel

### AC: input-thread-respects-pause

Given the input thread is running and `input_paused` is `true`
When the user presses keys
Then the input thread does not read from stdin and instead sleeps for 50ms per iteration, discarding no events but generating none

### AC: file-watcher-md-hot-reload

Given the file watcher is active on a document type directory
When a `.md` file is created, modified, or removed
Then `store.reload_file` is called for that specific path, the expansion and disk caches for that path are invalidated, and validation is refreshed

### AC: file-watcher-non-md-cache-clear

Given the file watcher is active on a document type directory
When a non-`.md` file changes
Then all expansion body caches and disk caches are cleared entirely, and validation is refreshed

### AC: editor-pause-protocol

Given the user triggers an editor launch (e.g., pressing `e` on a document)
When the main thread processes the `editor_request`
Then it sets `input_paused` to `true`, drains the channel, leaves alternate screen, spawns the editor process, and on return re-enters alternate screen, drains stale events, sets `input_paused` to `false`, and reloads the edited file

### AC: editor-resolution-order

Given the user triggers an editor launch
When the editor binary is resolved
Then `$EDITOR` is checked first, then `$VISUAL`, with `vi` as the fallback

### AC: probe-thread-detects-protocol

Given the TUI has started
When the probe thread executes `create_picker`
Then it queries the terminal for image protocol support via `Picker::from_query_stdio()` and falls back to `Picker::halfblocks()` on failure

### AC: probe-result-replaces-defaults

Given the probe thread has completed
When `AppEvent::ProbeResult` is received by the main loop
Then the app's picker, `terminal_image_protocol`, and `tool_availability` are replaced, and the diagram cache and image states are cleared

### AC: main-loop-drains-events

Given multiple events arrive on the channel between draw cycles
When the main loop receives the first event via `recv_timeout`
Then it continues draining with `try_recv` until the channel is empty before proceeding to the next draw

### AC: watcher-skips-missing-dirs

Given a document type is configured whose directory does not exist on disk
When the watcher registers directories at startup
Then that directory is silently skipped without error

### AC: shutdown-restores-terminal

Given the user quits the TUI (e.g., pressing `q`)
When the main loop exits
Then raw mode is disabled, the alternate screen is left, and the cursor is restored
