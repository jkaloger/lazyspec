---
title: "TUI Lifecycle and Threading"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, threading, lifecycle]
related:
  - related-to: docs/stories/STORY-003-tui-dashboard.md
  - related-to: docs/stories/STORY-073-tui-async-document-creation.md
---

## Summary

The TUI application follows a multi-threaded architecture where a single main thread owns all mutable state and rendering, while dedicated threads handle terminal input, filesystem watching, and one-shot capability probing. All inter-thread communication flows through a single unbounded `crossbeam_channel`, with events dispatched by variant in `handle_app_event`.

## Initialization

@ref src/tui/infra/event_loop.rs#run

The `run` function is the TUI entrypoint. It enables raw mode, enters the alternate screen, and constructs a ratatui `Terminal` backed by crossterm. An `App` is created with an initial halfblocks `Picker` (a cheap fallback that does not require terminal queries), then validation is run against the loaded store.

@ref src/tui/state/app.rs#App

After `App` construction, `run` creates the unbounded crossbeam channel and stores the sender on `app.event_tx`, making it available for background work spawned later (expansion workers, diagram renderers, async document creation).

## Threading Model

Three persistent threads exist for the lifetime of the application, plus a one-shot probe thread that exits after sending a single event.

### Main Thread

The main thread owns the `App` struct, the ratatui `Terminal`, and the crossbeam receiver. Each iteration of the main loop performs: draw, request background expansion and diagram renders, then `recv_timeout` on the channel for 16ms. When events arrive, it drains them all via `try_recv` before proceeding to post-loop checks (editor launch, fix request, quit).

@ref src/tui/infra/event_loop.rs#handle_app_event

### Input Thread

A dedicated thread calls `crossterm::event::read()` in a blocking loop and forwards `KeyEvent` values as `AppEvent::Terminal` through the channel. The thread checks an `AtomicBool` (`input_paused`) on each iteration; when the flag is set, it sleeps for 50ms instead of reading, preventing stale keystrokes from accumulating while an external process holds the terminal.

### File Watcher Thread

The `notify` crate spawns its own OS thread internally. `run` creates a `recommended_watcher` (inotify on Linux, kqueue on macOS) and registers each document type directory from the config as a non-recursive watch. The watcher callback sends `AppEvent::FileChange` through the channel.

### Probe Thread

A one-shot thread launched at startup that detects terminal image protocol support and diagram tool availability. It calls `create_picker`, which queries the terminal via `Picker::from_query_stdio` and falls back to halfblocks on failure. The result arrives as `AppEvent::ProbeResult`, which replaces the initial halfblocks picker and populates `terminal_image_protocol` and `tool_availability` on the app.

@ref src/tui/infra/terminal_caps.rs#create_picker

## Channel Architecture

@ref src/tui/state/app.rs#AppEvent

All threads communicate through a single unbounded `crossbeam_channel`. The `AppEvent` enum has these variants:

- `Terminal(KeyEvent)` -- keyboard input from the input thread
- `FileChange(notify::Event)` -- filesystem events from the watcher
- `ExpansionResult` -- completed `@ref` expansion from a background worker
- `DiagramRendered` -- rendered diagram image from a background worker
- `ProbeResult` -- terminal capability detection result from the probe thread
- `CreateStarted`, `CreateProgress`, `CreateComplete` -- async document creation lifecycle
- `AgentFinished` -- (behind `agent` feature flag) signals an agent session completed

The main loop calls `recv_timeout(Duration::from_millis(16))` to wait for the first event, then `try_recv` in a loop to drain any additional events that arrived during processing, before proceeding to the next draw cycle.

## Input Pause Protocol

When the main thread needs to hand terminal control to an external process (editor or Claude agent), it follows a specific pause protocol:

@ref src/tui/infra/event_loop.rs#run_editor

1. Set `input_paused` to `true` on the `AtomicBool`, causing the input thread to spin-sleep instead of reading keys.
2. Drain the channel with `try_recv` to discard any in-flight events.
3. Leave the alternate screen and disable raw mode.
4. Spawn and wait on the external process (`$EDITOR` for editing, `claude --resume` for agent sessions).
5. Re-enable raw mode and re-enter the alternate screen.
6. Drain the channel again to discard stale file-change events that may have arrived.
7. Set `input_paused` to `false`.
8. Reload affected documents and refresh validation.

@ref src/tui/state/app.rs#resolve_editor

Editor resolution checks `$EDITOR`, then `$VISUAL`, falling back to `vi`.

## File Watcher and Hot-Reload

The watcher distinguishes between Markdown and non-Markdown file changes. For `.md` files, `store.reload_file` is called for the specific path, and the expansion and disk caches for that path are invalidated. For non-`.md` file changes (source code referenced by `@ref` directives), all expansion caches are cleared because any ref could now be stale. In both cases, `refresh_validation` is called and the git status cache is invalidated.

Watch registration is non-recursive: only the top-level directory for each document type is watched, not subdirectories. Directories that do not exist on disk are silently skipped.

## Terminal Capability Detection

@ref src/tui/infra/terminal_caps.rs#TerminalImageProtocol

The `TerminalImageProtocol` enum represents the detected image rendering capability: `Sixel`, `KittyGraphics`, `Iterm2`, `Halfblocks`, or `Unsupported`. The app starts with a `Halfblocks` picker as a safe default, then replaces it when `ProbeResult` arrives from the probe thread.

`create_picker` calls `Picker::from_query_stdio()`, which writes escape sequences to stdout and reads the terminal's response to determine protocol support. If the query fails (piped output, unsupported terminal), it falls back to `Picker::halfblocks()`. When the probe result arrives, the app also resets its diagram cache and image states, since the rendering protocol may have changed.

## Shutdown

The main loop exits when `app.should_quit` is `true`. Cleanup disables raw mode, leaves the alternate screen, and restores the cursor. There is no explicit thread join; the input thread and watcher thread are detached and terminate when the process exits.
