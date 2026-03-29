# Convention & Dictums Design — Lazyspec Codebase

## Overview

A set of codebase conventions for the lazyspec project, targeting AI agents as the primary consumer while remaining human-readable. Delivered through lazyspec's own convention/dictum system.

## Audience

Primarily AI agents consuming conventions via `lazyspec convention --json` and `lazyspec convention --preamble`. Human-readable as project documentation.

## Structure

One **convention** (singleton preamble) + seven **dictums** (broad themes, one per major concern). Each dictum is tagged on two axes: concern and architectural layer.

### Tag Axes

**Concern tags:** `rust`, `style`, `traits`, `module-structure`, `architecture`, `testing`, `tech-stack`, `patterns`

**Layer tags:** `engine`, `cli`, `tui`

## Convention Preamble

Lazyspec is a Rust CLI/TUI tool for managing structured project documentation as version-controlled markdown. It's a single binary (engine + CLI + TUI) built for both human and agent consumption. The codebase values idiomatic Rust, clear module boundaries, and testability through trait-based abstractions. All CLI output supports `--json` for agent integration. The TUI is built on ratatui. Documentation is the product — the codebase dogfoods itself.

## Dictums

### 1. Idiomatic Rust

**Tags:** `rust`, `style`, `engine`, `cli`, `tui`

- `anyhow::Result<T>` for all fallible functions — no custom error types unless a caller needs to match on variants
- Propagate errors with `?`, add context with `.context()` / `.with_context()` when the call site wouldn't be obvious from a stack trace
- Prefer owned types in structs, borrow in function signatures where the lifetime is obvious
- Standard Rust naming: `snake_case` functions/variables, `PascalCase` types, `SCREAMING_SNAKE` constants
- Use `impl Into<T>` / `AsRef<T>` for flexible public APIs, concrete types for internal code
- No `unwrap()` outside of tests. `expect()` only when the invariant is genuinely guaranteed and the message explains why
- Prefer iterators and combinators over manual loops where readability doesn't suffer
- Derive `Debug` on all public types. Derive `Clone`, `PartialEq` etc. when there's a use for it, not speculatively
- Prefer `&str` over `&String` in function parameters
- Use `Default` trait and derive it where sensible — prefer `Type::default()` over manual field-by-field construction
- Prefer `if let` / `let else` over `match` when only one variant matters
- Use `From`/`Into` implementations for type conversions rather than ad-hoc methods
- Prefer `collect()` into concrete types over building collections manually
- Avoid `clone()` as a first resort — restructure ownership first, clone when the borrow checker fight isn't worth it
- Use tuple structs / newtypes for domain concepts (like `DocType(String)`) rather than bare primitives
- Prefer exhaustive `match` over `_` wildcards when the enum is local — forces handling new variants

### 2. Trait Usage

**Tags:** `traits`, `rust`, `engine`, `cli`, `tui`

- Use traits for **testability boundaries** — the `FileSystem` trait pattern is the canonical example: real I/O in prod, injectable in tests
- Use traits for **polymorphism you actually need** — e.g., multiple store backends (filesystem, GitHub Issues)
- Don't introduce a trait for a single implementation. If there's only one impl and no testing seam, use a concrete type
- Trait methods should be minimal — prefer several small traits over one fat trait. A consumer should never need to implement methods it doesn't use
- Default implementations are fine when there's an obvious sensible default, not as a way to make a big trait look smaller
- Prefer static dispatch (`impl Trait` / generics) for internal code. Use `dyn Trait` when you need heterogeneous collections or the type must be erased
- Keep trait definitions in the module that owns the concept, not in the consumer. `FileSystem` lives in `engine/fs.rs`, not in the test module
- When a trait exists for testability, the mock/fake belongs in `#[cfg(test)]` of the consuming module, not next to the trait definition

### 3. Module Structure

**Tags:** `module-structure`, `architecture`, `engine`, `cli`, `tui`

- Three top-level crates-within-a-crate: `engine/` (core logic, no I/O assumptions), `cli/` (command dispatch, output formatting), `tui/` (ratatui state and rendering)
- `engine` knows nothing about `cli` or `tui`. `cli` and `tui` depend on `engine`. `cli` and `tui` don't depend on each other
- One file per concern — `store.rs` does store loading, `validation.rs` does validation, `refs.rs` does ref parsing. Don't stuff unrelated logic into an existing file because it's nearby
- Each CLI command gets its own module under `cli/` exporting a `run()` function (and `run_json()` if it supports JSON output)
- Public API surface of `engine/` is the contract — minimize `pub` exports, keep internal helpers private. If a `cli` module needs something from `engine`, that's a signal to think about whether the API is right
- `lib.rs` re-exports the three top-level modules. Don't add logic to `lib.rs` or `main.rs` — they're wiring only
- When a module grows past ~400 lines, that's a signal to split. Factor out a sub-module, don't just keep appending
- Use the file-as-module pattern (`foo.rs` + `foo/` directory), not `foo/mod.rs`. This keeps the module declaration visible at the parent level
- Sub-modules go in the directory: `engine/store.rs`, `engine/refs.rs`. Further nesting follows the same pattern: `engine/store.rs` + `engine/store/loader.rs`
- New document types, validation rules, numbering strategies — follow the existing pattern. Look at how the last one was added before inventing a new structure

### 4. Testing

**Tags:** `testing`, `engine`, `cli`, `tui`

**Desiderata** (Kent Beck) — applied as actionable constraints:

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

**Organization:**

- Unit tests in `#[cfg(test)] mod tests` at the bottom of the source file — focused logic within a single module
- Integration tests in `tests/` — CLI commands, TUI state machines, cross-module behavior. This is where most coverage lives
- Shared fixtures and helpers in `tests/common/mod.rs`
- Use `tempfile::TempDir` for any test that touches the filesystem

**Practice:**

- Prefer real types over mocks. Use trait seams (e.g., `FileSystem`) when you need to control I/O, not as the default
- TUI tests exercise state transitions and key handling through the public state API
- CLI integration tests set up a temp project, run command logic, assert on output
- Descriptive test names that convey the scenario being verified

### 5. Tech Stack

**Tags:** `tech-stack`, `engine`, `cli`, `tui`

- **Rust 2021 edition**, single binary architecture — engine, CLI, and TUI share one crate
- **anyhow** for error handling — `anyhow::Result<T>` everywhere, no custom error enums unless callers need to match variants
- **clap 4** with derive macros for CLI parsing — every command is a variant on the `Commands` enum
- **serde** + **serde_yaml** for frontmatter, **toml** for config, **serde_json** for `--json` output
- **ratatui** + **crossterm** for TUI — ratatui owns rendering, crossterm owns terminal events
- **tree-sitter** with language grammars (Rust, TypeScript) for symbol extraction in `@ref` directives
- **tempfile** for test fixtures
- **pulldown-cmark** for markdown parsing
- **sqids** for hash-based document numbering
- **indicatif** for progress bars in CLI
- **crossbeam-channel** for TUI event loop threading
- When adding a dependency, prefer crates already in use. Don't introduce a new crate for something an existing dependency already handles
- Feature flags (`agent`, `metrics`) gate optional functionality — don't put feature-gated code behind runtime checks when compile-time gating works

### 6. CLI Patterns

**Tags:** `cli`, `patterns`

- Every command gets its own module under `cli/` — `cli/show.rs`, `cli/validate.rs`, etc.
- Each module exports a `run()` function. If the command supports `--json`, it also exports `run_json()` or handles the flag internally
- Every command that produces output must support `--json` for agent consumption. This is non-negotiable — agents are first-class consumers
- Use clap derive macros for argument definitions. Keep the `Commands` enum in `cli.rs` as the single dispatch point
- Output formatting goes through `cli/style.rs` — don't inline ANSI codes in command modules
- `main.rs` does wiring only: parse args, load store, match command, call `run()`. No logic lives there
- Document ID arguments should go through the existing resolution/fuzzy matching — don't hand-roll ID parsing in individual commands
- Errors surface to the user as human-readable messages. Don't print raw `Debug` output. `anyhow` context messages should be written for the person reading them
- JSON output schemas should be consistent across commands — use the serialization patterns in `cli/json.rs`, don't invent per-command JSON shapes

### 7. TUI Patterns

**Tags:** `tui`, `patterns`

- State and rendering are separate concerns — `tui/state/` owns application state and transitions, `tui/views/` owns how state gets drawn. Views read state, they don't mutate it
- Key handling produces state transitions, not side effects. A keypress updates state, the next render cycle reflects it
- The event loop in `tui/infra/event_loop.rs` is the single driver — it reads crossterm events, dispatches to state, and triggers renders. Don't add secondary event sources outside this loop
- Widgets are ratatui widgets — compose them, don't fight the framework. If you need custom rendering, implement `Widget` rather than drawing raw cells
- Colors and theming go through `tui/views/colors.rs` — don't hardcode color values in view modules
- TUI state is testable without a terminal — tests construct state, call transition methods, and assert on the resulting state. No terminal needed, no frame rendering in tests
- Overlays (dialogs, pickers, forms) are state variants, not separate widget trees. The view layer checks what overlay is active and renders accordingly
- `crossbeam-channel` handles communication between the event loop and background work. Don't spawn threads that mutate state directly — send messages through the channel
- Feature-gated code (`agent`) should be behind `#[cfg(feature = "...")]`, not runtime flags
