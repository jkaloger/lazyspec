---
title: Shell Completions
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-070-shell-completions.md
---



Covers all 10 ACs from STORY-070.

## Changes

### Task 1: Add clap_complete dependency and `completions` subcommand with static generation

**ACs addressed:** AC1, AC2, AC3, AC4, AC5, AC6

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`
- Create: `tests/cli_completions_test.rs`

**What to implement:**

Add `clap_complete` dependency to `Cargo.toml`:
```toml
clap_complete = "4"
```

Add a `Completions` variant to the `Commands` enum in `src/cli/mod.rs`:
```rust
/// Generate shell completion scripts
Completions {
    /// Shell to generate completions for
    #[arg(value_enum)]
    shell: clap_complete::Shell,
}
```

`clap_complete::Shell` already implements `ValueEnum` and supports `Bash`, `Zsh`, `Fish`, `Elvish`, `PowerShell`. The `value_enum` attribute gives clap automatic validation and error messages for unsupported values (AC4).

Handle the new subcommand in `src/main.rs`, before the config/store load (like `Init`), since static completions don't need the store:
```rust
if let Some(Commands::Completions { shell }) = &cli.command {
    clap_complete::generate(*shell, &mut Cli::command(), "lazyspec", &mut std::io::stdout());
    return Ok(());
}
```

This uses `clap::CommandFactory` (auto-derived via `Parser`) to generate completion scripts that include all subcommands (AC5) and flags (AC6).

**How to verify:**
- `cargo test --test cli_completions_test` -- tests cover: zsh/bash/fish each produce non-empty output containing "lazyspec", unsupported shell name gives error exit.

---

### Task 2: Dynamic document ID and relationship type completions via CompleteEnv

**ACs addressed:** AC7, AC8, AC9, AC10

**Files:**
- Modify: `Cargo.toml` (add `clap_complete` feature flag if needed)
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`
- Create: `src/cli/completions.rs`
- Add tests to: `tests/cli_completions_test.rs`

**What to implement:**

`clap_complete` 4.x provides the `CompleteEnv` mechanism for dynamic, runtime completions. The shell calls back into the binary at completion time via environment variables (e.g. `_CLAP_COMPLETE`). This means completions are always live (AC9) without regenerating scripts.

**Step 1: Add `add(clap_complete::CompleteEnv)` hook in `src/main.rs`:**

Before `Cli::parse()`, add:
```rust
clap_complete::CompleteEnv::with_factory(Cli::command)
    .completer(LazyspecCompleter)
    .complete();
```

This intercepts the completion protocol before normal CLI execution. If the shell is requesting completions, it responds and exits. Otherwise, normal execution continues.

**Step 2: Create `src/cli/completions.rs` with a custom completer:**

Create a struct `LazyspecCompleter` that implements `clap_complete::CustomCompleter`. This completer provides dynamic candidates for:

1. **Document ID arguments** (AC7): When completing an arg that accepts a document reference (the positional args on `show`, `link`, `context`, `delete`, `update`, `ignore`, `unignore`), load the store and return `doc.id` for all documents. Use `Store::load()` with `Config::load()` from cwd. Since `Store::all_docs()` returns `Vec<&DocMeta>` and each has an `id: String` field, collect these as completion candidates.

2. **Relationship type argument** (AC8): When completing the `rel_type` positional on `link`/`unlink`, return the four known values: `implements`, `supersedes`, `blocks`, `related-to`.

**Step 3: Annotate args with custom completers in `src/cli/mod.rs`:**

Use `#[arg(add = ValueCompleter::new(...))]` or the `clap_complete` annotation mechanism to attach the custom completer to the relevant args. The exact API depends on the clap_complete 4.x version; check the docs during implementation. The key is that document-reference positional args get the doc ID completer, and `rel_type` args get the relationship type completer.

**Step 4: Graceful degradation (AC10):**

In the completer, wrap `Config::load()` and `Store::load()` in a `match` or `.ok()`. If either fails (unreadable store, no config, corrupted data), return an empty `Vec` of candidates. Static completions (subcommands, flags) still work because they come from clap's derive metadata, not the custom completer.

**How to verify:**
- `cargo test --test cli_completions_test` -- tests cover: completer returns doc IDs from a test fixture, completer returns relationship types, completer returns empty on missing/broken store.
- Manual verification: source the generated script in a shell and confirm tab completion works for doc IDs.

## Test Plan

All tests in `tests/cli_completions_test.rs`.

| # | AC | Test | Tradeoffs |
|---|-----|------|-----------|
| 1 | AC1 | Run `completions zsh` via CLI, assert output is non-empty and contains "lazyspec" and "_arguments" (zsh-specific) | Fast, deterministic |
| 2 | AC2 | Run `completions bash` via CLI, assert output is non-empty and contains "lazyspec" and "complete" (bash-specific) | Fast, deterministic |
| 3 | AC3 | Run `completions fish` via CLI, assert output is non-empty and contains "lazyspec" and "complete" (fish-specific) | Fast, deterministic |
| 4 | AC4 | Run binary with `completions invalid_shell`, assert non-zero exit and error message | Fast, behavioral |
| 5 | AC5+6 | Assert the generated zsh script contains known subcommand names and flag names | Trades structure-insensitivity for predictiveness (checks generated output format) |
| 6 | AC7 | Create a `TestFixture` with docs, instantiate the custom completer, assert it returns the expected shorthand IDs | Isolated, fast, deterministic |
| 7 | AC8 | Instantiate completer for rel_type context, assert it returns `["implements", "supersedes", "blocks", "related-to"]` | Isolated, fast |
| 8 | AC9 | Implicit: tests 6 and 7 exercise the live completer path which reads the store at invocation time, not from a cached script | No separate test needed; the CompleteEnv design guarantees this |
| 9 | AC10 | Instantiate completer with a non-existent or empty root dir, assert it returns empty vec (no panic, no error output) | Isolated, specific |

Tests 1-5 are integration-level (invoke the binary or generate output). Tests 6-9 are unit-level (call the completer directly). This trades Fast slightly for Predictive on the integration tests, which is appropriate since shell completion scripts have format-specific requirements.

## Notes

- The `CompleteEnv` approach means the generated completion script includes a callback to the binary itself. When the user tabs, the shell invokes `lazyspec` with special env vars, the `CompleteEnv` hook intercepts, loads the store, and returns candidates. This is why AC9 (new docs appear without regeneration) works automatically.
- `clap_complete::Shell` already handles the "unsupported shell" error case (AC4) via clap's value validation. No custom error handling needed.
- The `completions` subcommand (Task 1) generates the initial script the user sources. The `CompleteEnv` hook (Task 2) handles the runtime callback. Both are needed.
- The exact `clap_complete` 4.x API for custom completers may require checking the latest docs during implementation. The `CustomCompleter` trait or `ValueCompleter` type is the entry point.
