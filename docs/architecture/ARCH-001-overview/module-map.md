---
title: "Module Map"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, overview]
related: []
---

# Module Map

```
src/
  main.rs                 Entry point, CLI routing
  lib.rs                  Library re-exports

  engine/
    mod.rs                Module declarations
    config.rs             .lazyspec.toml parsing, defaults
    document.rs           DocMeta, Status, RelationType, frontmatter
    store.rs              Document index, links, search, hot-reload
    validation.rs         Rule engine, issue types
    refs.rs               @ref directive expansion via git
    symbols.rs            Tree-sitter symbol extraction (TS + Rust)
    cache.rs              Disk cache for expanded refs
    template.rs           Filename generation, slugification

  cli/
    mod.rs                Clap definitions (Cli, Commands)
    init.rs               lazyspec init
    create.rs             lazyspec create
    list.rs               lazyspec list
    show.rs               lazyspec show
    update.rs             lazyspec update
    delete.rs             lazyspec delete
    link.rs               lazyspec link / unlink
    search.rs             lazyspec search
    status.rs             lazyspec status
    context.rs            lazyspec context (chain traversal)
    validate.rs           lazyspec validate
    fix.rs                lazyspec fix (auto-repair)
    ignore.rs             lazyspec ignore / unignore
    json.rs               JSON serialization helpers
    style.rs              Terminal colors and formatting

  tui/
    mod.rs                Event loop, file watcher, editor integration
    app.rs                App state, key handling, all UI state machines
    ui.rs                 Ratatui rendering (all view modes + overlays)
    diagram.rs            d2/mermaid block extraction and rendering
    terminal_caps.rs      Terminal image protocol detection
    agent.rs              Claude agent spawner (feature-gated)
```

## Key Dependencies

| Crate | Purpose |
|---|---|
| `clap` (4, derive) | CLI argument parsing |
| `ratatui` (0.30) + `crossterm` (0.28) | Terminal UI framework |
| `serde` + `serde_yaml` + `serde_json` | Serialization |
| `tree-sitter` (0.24) | Code parsing for symbol extraction |
| `tree-sitter-rust`, `tree-sitter-typescript` | Language grammars |
| `notify` (7) | Filesystem watching |
| `crossbeam-channel` | Multi-producer event channel |
| `ratatui-image` (10) | Terminal image rendering (sixel/kitty/iterm2) |
| `chrono`, `regex`, `toml`, `anyhow` | Utilities |
