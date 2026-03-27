---
title: "Trait Usage"
type: dictum
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: [architecture, build, testing]
---


Traits are used for I/O abstraction boundaries, not for general polymorphism. The canonical example is `FileSystem` in `src/engine/fs.rs`, which defines 7 methods (`read_to_string`, `write`, `rename`, `read_dir`, `exists`, `create_dir_all`, `is_dir`). Two implementations exist: `RealFileSystem` for production and `InMemoryFileSystem` for unit tests.

Functions that touch the filesystem accept `&dyn FileSystem`. This is the seam that makes the engine testable without disk I/O. The `Store::load()` convenience function hardcodes `RealFileSystem`; `Store::load_with_fs()` accepts an arbitrary implementation.

Derive `serde::{Serialize, Deserialize}` on config and document types. Implement `Display` and `Error` manually on domain error enums where callers need to pattern-match on variants. Derive `Default` on config structs where zero-values are meaningful.

Do not introduce trait abstractions for TUI components or CLI handlers. Views are concrete functions. CLI handlers are free functions. Traits add indirection; use them only when you need runtime dispatch across multiple implementations (typically for I/O boundaries or plugin points).
