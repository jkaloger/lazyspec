---
title: "Event Loop"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, tui]
related: []
---

# Event Loop

```d2
direction: down

start: "App::new(store, config)" {
  shape: parallelogram
}

draw: "terminal.draw(ui::draw)" {
  style.fill: "#e8f0fe"
}

request: "request_expansion(tx)" {
  desc: "Queue ref expansion if not cached"
}

diagrams: "request_diagram_render()" {
  desc: "Queue diagram renders for visible blocks"
}

recv: "rx.recv_timeout(100ms)" {
  shape: diamond
}

handle: "handle_app_event()" {
  terminal: "Terminal -> app.handle_key()"
  file: "FileChange -> reload_file() + refresh_validation()"
  expansion: "ExpansionResult -> cache + insert"
  diagram: "DiagramRendered -> diagram_cache.insert()"
  probe: "ProbeResult -> update protocol + tools"
}

editor_check: "editor_request?" {
  shape: diamond
}

editor: "run_editor()" {
  style.fill: "#fff3e0"
}

fix_check: "fix_request?" {
  shape: diamond
}

fix: "cli::fix::run_human()" {
  desc: "Auto-repair, reload store"
}

quit_check: "should_quit?" {
  shape: diamond
}

done: "Cleanup + exit" {
  shape: parallelogram
}

start -> draw -> request -> diagrams -> recv
recv -> handle: "event received"
recv -> editor_check: "timeout"
handle -> editor_check
editor_check -> editor: "Some(path)"
editor_check -> fix_check: "None"
editor -> fix_check
fix_check -> fix: "true"
fix_check -> quit_check: "false"
fix -> quit_check
quit_check -> done: "true"
quit_check -> draw: "false (loop)"
```

The event dispatch function:

@ref src/tui/mod.rs#handle_app_event

## File Watching

The file watcher monitors all configured type directories (non-recursive) using
the `notify` crate. On file events:

- **.md files**: Hot-reload the specific document via `store.reload_file()`,
  invalidate expansion cache, refresh validation
- **Non-.md files**: Clear all expansion caches (source code changed, refs may
  be stale), refresh validation
