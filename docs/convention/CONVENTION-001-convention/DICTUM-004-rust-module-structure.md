---
title: "Rust Module Structure"
type: dictum
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: [architecture, build]
---


`src/lib.rs` re-exports three top-level modules: `engine`, `cli`, `tui`. Each top-level module file (`src/engine.rs`, `src/cli.rs`, `src/tui.rs`) declares its submodules with `pub mod`.

Use private submodules for internal decomposition. If a module grows large, split it into private child modules (`mod links; mod loader;` without `pub`) that are not visible outside the parent. Only make a submodule `pub` if external code needs to import from it.

Use `pub(crate)` for struct fields and functions that need cross-module access within the crate but should not be part of the public API. The `Store` fields follow this pattern.

Re-exports should be targeted: `pub use specific::Item;` at the module boundary. No glob re-exports (`pub use foo::*`) in production code.

Feature-gated modules use `#[cfg(feature = "name")]` on the `pub mod` declaration. The TUI's `agent` module follows this pattern.
