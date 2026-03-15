---
title: "Threading Model"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, tui, threading]
related:
  - related-to: "docs/stories/STORY-017-open-in-editor.md"
---

# Threading Model

The TUI runs three threads plus optional background workers:

@ref src/tui/mod.rs#run

```d2
direction: right

main_thread: "Main Thread" {
  style.fill: "#e8f0fe"
  draw: "terminal.draw()"
  dispatch: "handle_app_event()"
  expansion_req: "request_expansion()"
  editor: "run_editor() [blocks]"
}

input_thread: "Input Thread" {
  style.fill: "#e6f4ea"
  poll: "crossterm::event::poll(50ms)"
  read: "crossterm::event::read()"
  pause: "input_paused: AtomicBool"
}

watcher_thread: "File Watcher Thread" {
  style.fill: "#fff3e0"
  notify: "notify::recommended_watcher"
  dirs: "watches type directories"
}

probe_thread: "Probe Thread (one-shot)" {
  style.fill: "#f3e5f5"
  picker: "detect terminal protocol"
  tools: "check d2/mmdc availability"
}

expansion_worker: "Expansion Worker (spawned per doc)" {
  style.fill: "#fce4ec"
  ref_expand: "RefExpander::expand_cancellable()"
  cancel: "AtomicBool cancel flag"
}

channel: "crossbeam_channel\n(unbounded)" {
  shape: queue
}

input_thread -> channel: "AppEvent::Terminal"
watcher_thread -> channel: "AppEvent::FileChange"
probe_thread -> channel: "AppEvent::ProbeResult"
expansion_worker -> channel: "AppEvent::ExpansionResult"
channel -> main_thread: "recv_timeout(100ms)"
```

## Event Types

@ref src/tui/app.rs#AppEvent

## Input Pausing

When launching an external editor or Claude session, the input thread is paused
via an `AtomicBool` flag. The main thread:

1. Sets `input_paused = true`
2. Drains the event channel
3. Leaves alternate screen, disables raw mode
4. Spawns the external process and waits
5. Re-enters alternate screen, enables raw mode
6. Drains stale events
7. Sets `input_paused = false`
8. Reloads affected documents

See [STORY-017: Open in Editor](../../stories/STORY-017-open-in-editor.md) for the
editor integration that drives this pattern.
