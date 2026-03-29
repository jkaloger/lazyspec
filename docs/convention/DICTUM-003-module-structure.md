---
title: "Module Structure"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [module-structure, architecture, engine, cli, tui]
---

## Top-Level Architecture

- Three crates-within-a-crate: `engine/` (core logic, no I/O assumptions), `cli/` (command dispatch, output formatting), `tui/` (ratatui state and rendering)
- `engine` knows nothing about `cli` or `tui`. `cli` and `tui` depend on `engine`. `cli` and `tui` don't depend on each other
- `lib.rs` re-exports the three top-level modules. Don't add logic to `lib.rs` or `main.rs` — they're wiring only

## File Organization

- One file per concern — `store.rs` does store loading, `validation.rs` does validation, `refs.rs` does ref parsing. Don't stuff unrelated logic into an existing file because it's nearby
- Each CLI command gets its own module under `cli/` exporting a `run()` function (and `run_json()` if it supports JSON output)
- When a module grows past ~400 lines, that's a signal to split. Factor out a sub-module, don't just keep appending

## Module Style

- Use the file-as-module pattern (`foo.rs` + `foo/` directory), not `foo/mod.rs`. This keeps the module declaration visible at the parent level
- Sub-modules go in the directory: `engine/store.rs`, `engine/refs.rs`. Further nesting follows the same pattern: `engine/store.rs` + `engine/store/loader.rs`

## API Surface

- Public API surface of `engine/` is the contract — minimize `pub` exports, keep internal helpers private. If a `cli` module needs something from `engine`, that's a signal to think about whether the API is right

## Conventions

- New document types, validation rules, numbering strategies — follow the existing pattern. Look at how the last one was added before inventing a new structure
