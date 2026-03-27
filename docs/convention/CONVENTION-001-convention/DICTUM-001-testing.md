---
title: "Testing"
type: dictum
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: [testing, build, iteration]
---


Tests use the standard Rust test harness. No external test frameworks.

Integration tests in `tests/` import `lazyspec` as a library crate. Unit tests live inline as `#[cfg(test)] mod tests` inside source files. Integration tests are the dominant pattern and cover CLI commands, validation, store loading, and TUI state transitions.

All integration tests use the shared `TestFixture` from `tests/common/mod.rs`. The fixture wraps a `tempfile::TempDir` and pre-creates the standard document directory tree. Use the typed helpers (`write_rfc`, `write_story`, `write_iteration`) rather than writing raw frontmatter by hand.

For filesystem-sensitive unit tests that need to avoid disk I/O, use `InMemoryFileSystem` (defined in `src/engine/store.rs` tests) which implements the `FileSystem` trait over a mutex-guarded `HashMap`.

Test naming: integration tests use bare `snake_case` without a `test_` prefix (e.g., `fn validate_catches_broken_link()`). Inline unit tests use the `test_` prefix (e.g., `fn test_expand_refs_single_ref()`).

Error paths are tested with `assert!(result.is_err())` and pattern matching on the error message. Never use `#[should_panic]`.

Tests should be behavioral (assert on observable output, not internal state), isolated (no shared mutable state between tests), and deterministic (no timing, randomness, or ordering dependencies). When writing an integration test that sacrifices speed for predictive value, note the tradeoff.
