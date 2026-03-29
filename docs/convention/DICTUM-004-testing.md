---
title: "Testing"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [testing, engine, cli, tui]
---

## Desiderata

Kent Beck's test desiderata, applied as actionable constraints:

- **Isolated** — each test creates its own state (own `TempDir`, own `Store` instance). Never depend on state left by another test. No shared mutable statics
- **Composable** — no test should require another test to run first. No numbered test ordering. If you need shared setup, put it in a helper that each test calls independently
- **Fast** — no sleeps, no network calls, no spawning processes. If a test needs I/O, use the `FileSystem` trait seam or `TempDir`
- **Inspiring** — test the things that would break in production. Don't write tests for trivial getters. A green suite should mean "this is safe to ship"
- **Writable** — if a test requires 50 lines of setup to test 1 line of behavior, the API is wrong. Fix the API, don't write the 50-line test
- **Readable** — a test should read as: arrange, act, assert. The test name states the scenario, the body shows it. No chasing through abstractions to understand what's being verified
- **Behavioral** — assert on what the code *does* (returns, produces, writes), not *how* it does it (which internal method was called, in what order)
- **Structure-insensitive** — test through public APIs. If you refactor internals and tests break, the tests were coupled to structure
- **Automated** — `cargo test` runs everything. No manual steps, no environment variables to set, no services to start
- **Specific** — one assertion per logical behavior. When a test fails, the name and assertion should tell you what broke without reading the implementation
- **Deterministic** — no random data, no timestamps, no system-dependent paths. Use fixed fixtures
- **Predictive** — if you're not confident the test would catch a real bug, it's not worth writing

## Organization

- Unit tests in `#[cfg(test)] mod tests` at the bottom of the source file — focused logic within a single module
- Integration tests in `tests/` — CLI commands, TUI state machines, cross-module behavior. This is where most coverage lives
- Shared fixtures and helpers in `tests/common/mod.rs`
- Use `tempfile::TempDir` for any test that touches the filesystem

## Practice

- Prefer real types over mocks. Use trait seams (e.g., `FileSystem`) when you need to control I/O, not as the default
- TUI tests exercise state transitions and key handling through the public state API
- CLI integration tests set up a temp project, run command logic, assert on output
- Descriptive test names that convey the scenario being verified
