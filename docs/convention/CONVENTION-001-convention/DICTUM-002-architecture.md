---
title: "Architecture"
type: dictum
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: [architecture, rfc, build]
---



The codebase has three layers with a strict dependency direction:

`engine` contains all domain logic: config parsing, document loading, validation, relationship resolution, template expansion, and content hashing. It has no knowledge of CLI arguments or terminal rendering. Functions accept `&dyn FileSystem` for I/O, making the engine testable without disk access.

`cli` contains one file per subcommand. Each exposes free functions (`run`, `run_json`, `run_human`) that accept a loaded `Store` and `Config`. CLI handlers are thin: they call engine functions and format output. They hold no state.

`tui` contains the Ratatui-based interactive UI. It enters via `tui::run(store, &config)` and owns the event loop, rendering, and input handling. The TUI holds `Store` by value inside `App` and calls `store.reload_file(...)` on file-watch events.

`main.rs` is the integration point: parse CLI, load config and store, dispatch to the appropriate handler or fall through to the TUI.

New features should respect this layering. Business rules belong in `engine`. If you find yourself importing `cli` or `tui` types from `engine`, the dependency is going the wrong direction.
