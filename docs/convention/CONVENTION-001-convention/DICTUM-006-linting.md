---
title: "Linting"
type: dictum
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: [build, iteration]
---


Run `cargo clippy` before considering a task complete. The project does not currently enforce clippy lints at the configuration level (no `clippy.toml`, no `[lints]` in `Cargo.toml`, no `#[deny]` attributes in source), but all clippy warnings should be resolved before committing.

The only lint suppression in the codebase is `#![allow(dead_code, unused_imports)]` in `tests/common/mod.rs`, which prevents noise from the shared test helper module where not every helper is used by every test file.

Do not add `#[allow(...)]` attributes to suppress warnings in production code. If clippy flags something, fix it. If the fix is genuinely worse than the warning (rare), document why in a comment next to the allow attribute.

Run `cargo test` after `cargo clippy` to confirm no regressions. The full test suite should pass clean before any commit.
