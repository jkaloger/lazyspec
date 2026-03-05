---
title: Styled CLI Output
type: iteration
status: draft
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-023-styled-cli-output.md
---


## Changes

### Task 1: Add `console` crate and create `src/cli/style.rs`

**ACs addressed:** AC-1 (colored status badges), AC-6 (plain-text fallback)

**Files:**
- Modify: `Cargo.toml`
- Create: `src/cli/style.rs`
- Modify: `src/cli/mod.rs`

**What to implement:**

Add `console = "0.15"` to `[dependencies]` in Cargo.toml. The `console` crate provides `Style`, `Color`, ANSI stripping, and automatic terminal capability detection (respects `NO_COLOR` env, piped output, dumb terminals).

Create `src/cli/style.rs` with these shared primitives:

- `status_style(status: &Status) -> console::Style` -- returns a Style with the appropriate foreground color: green for accepted, yellow for draft, blue for review, red for rejected, dark grey (color256 8) for superseded. These match the existing TUI color mapping in `src/tui/ui.rs`.
- `styled_status(status: &Status) -> String` -- applies `status_style` to the status Display string.
- `dim(text: &str) -> String` -- applies dim/faint styling.
- `bold(text: &str) -> String` -- applies bold styling.
- `type_header(doc_type: &DocType) -> String` -- renders a section header with a top border using box-drawing characters: `╭─ RFC ───────────────────╮` style. When colors are disabled, falls back to a plain `--- RFC ---` style header.
- `doc_card(title: &str, doc_type: &DocType, status: &Status, path: &Path) -> String` -- renders a single document row: bold title, colored status badge, dimmed path. Used by list/status/search.
- `separator() -> String` -- renders a thin horizontal rule `───`.
- `error_prefix() -> String` -- red bold "✗" (or "error:" when no color).
- `warning_prefix() -> String` -- yellow bold "!" (or "warning:" when no color).

All functions use `console::colors_enabled()` to decide whether to apply ANSI or return plain text. The `console` crate handles detection automatically based on terminal type and `NO_COLOR`.

Register `pub mod style;` in `src/cli/mod.rs`.

**How to verify:**
```
cargo test
cargo run -- status  # should show styled output in terminal
cargo run -- status | cat  # piped, should show plain output
NO_COLOR=1 cargo run -- status  # should show plain output
```

---

### Task 2: Style the `status` command

**ACs addressed:** AC-1, AC-2

**Files:**
- Modify: `src/cli/status.rs`

**What to implement:**

Rewrite `run_human()` to use the style primitives from `style.rs`:

- Each type group gets a `type_header()` instead of the plain `\n{}\n` format string.
- Each document row uses `doc_card()` -- bold title, colored status badge, dimmed path.
- Add a blank line between groups for breathing room.
- The overall structure remains: groups in order RFC, Story, Iteration, ADR.

The function signature stays the same (`pub fn run_human(store: &Store) -> String`). Import the style module.

**How to verify:**
```
cargo test --test cli_status_test
cargo run -- status
```

---

### Task 3: Style the `list` and `search` commands

**ACs addressed:** AC-1

**Files:**
- Modify: `src/cli/list.rs`
- Modify: `src/cli/search.rs`

**What to implement:**

**list.rs:** In the `run()` function's else branch (non-JSON), replace the plain `println!` with `doc_card()` for each document. Output one card per line.

**search.rs:** In the `run()` function's else branch:
- The document line uses `doc_card()` with the match field appended (dimmed, in brackets).
- The path line stays dimmed.
- The snippet line stays dimmed with the `...` prefix.
- The "No results" message stays plain.

**How to verify:**
```
cargo test --test cli_query_test
cargo run -- list
cargo run -- list rfc
cargo run -- search "auth"
```

---

### Task 4: Style the `show` command

**ACs addressed:** AC-1, AC-3

**Files:**
- Modify: `src/cli/show.rs`

**What to implement:**

Rewrite `run()` to use styled output:

- Title renders inside a bordered box using box-drawing characters: `╭─────────╮` / `│ Title   │` / `╰─────────╯` with bold text. When no color, use `# Title` as-is.
- Metadata line: dim labels ("Type:" "Status:" "Author:"), styled values (status gets color, type and author are bold).
- Tags line: dim "Tags:" label, each tag in its own style.
- A `separator()` between metadata and body.
- Body renders unchanged below.

The function signature stays the same.

**How to verify:**
```
cargo run -- show RFC-001
cargo run -- show STORY-023
```

---

### Task 5: Style `validate` and `context` commands

**ACs addressed:** AC-4, AC-5

**Files:**
- Modify: `src/cli/validate.rs`
- Modify: `src/cli/context.rs`

**What to implement:**

**validate.rs:** In `run_human()`:
- Errors use `error_prefix()` followed by the error message.
- Warnings use `warning_prefix()` followed by the warning message.
- "All documents valid." gets a green checkmark prefix when color is enabled.

**context.rs:** In `run_human()`:
- Each document in the chain renders as a mini card: bordered box with title (bold), type+status on a second line inside the box. Uses `type_header()`-style box drawing.
- The `↓` connector between documents becomes a styled vertical connector: `  │` with a dim color, or stays as `  ↓` in no-color mode.

**How to verify:**
```
cargo test --test cli_validate_test
cargo test --test cli_context_test
cargo run -- validate --warnings
cargo run -- context ITERATION-010
```

---

### Task 6: Update tests for styled output

**ACs addressed:** all (ensures tests pass with styling)

**Files:**
- Modify: `tests/cli_status_test.rs`
- Modify: `tests/cli_context_test.rs` (if assertions break)
- Modify: `tests/cli_validate_test.rs` (if assertions break)

**What to implement:**

The `run_human()` functions return Strings that now contain ANSI escape codes when colors are enabled. Tests run without a terminal, so `console` will auto-disable colors (piped/no-tty context). This means most tests should pass without changes.

However, verify this assumption. If any tests fail because `console` still emits ANSI in test context, add `console::set_colors_enabled(false)` at the start of affected tests, or use `console::strip_ansi_codes()` on the output before asserting.

The `status_human_grouped_by_type` test checks for `"RFC"`, `"STORY"`, `"ITERATION"` strings. If `type_header()` wraps these in box-drawing characters, the substring check still passes. But verify and adjust assertions if the header format changes the searchable text.

**How to verify:**
```
cargo test
```

## Test Plan

| Test | Verifies | Notes |
|------|----------|-------|
| Existing `status_human_grouped_by_type` | AC-2: grouped output still contains type names and document titles | May need assertion updates if header format changes |
| Existing `status_empty_project` | Empty state still returns empty/blank | Should be unaffected |
| Existing `cli_context_test` chain tests | AC-5: chain still contains document info | Verify connector change doesn't break assertions |
| Existing `cli_validate_test` tests | AC-4: error/warning output still parseable | Verify prefix change doesn't break assertions |
| Manual: `cargo run -- status` | AC-1, AC-2: colored badges, styled headers visible in terminal | Visual check |
| Manual: `cargo run -- status \| cat` | AC-6: piped output has no ANSI escapes | Visual check |
| Manual: `NO_COLOR=1 cargo run -- status` | AC-6: NO_COLOR respected | Visual check |
| Manual: `cargo run -- show RFC-001` | AC-3: bordered title, styled metadata | Visual check |
| Manual: `cargo run -- validate --warnings` | AC-4: red errors, yellow warnings | Visual check |
| Manual: `cargo run -- context ITERATION-001` | AC-5: styled chain cards with connectors | Visual check |

The styling output is inherently visual, so automated tests focus on content correctness (titles, statuses, paths are present) while manual checks verify the aesthetic. This trades some Predictive coverage for Writable/Fast tests, which is appropriate for a presentation-layer change.

## Notes

- The `console` crate was chosen over `owo-colors` because it bundles terminal detection, `NO_COLOR` support, and ANSI stripping in one package. No need for a separate `supports-color` crate.
- The TUI already maps statuses to colors in `src/tui/ui.rs`. The CLI style module replicates this mapping using `console::Color` instead of `ratatui::style::Color`. If these drift, a future refactor could extract a shared color palette, but that's out of scope here.
- Box-drawing characters (╭╮╰╯│─) are UTF-8 and render correctly in all modern terminals. Legacy terminals that can't render them will show fallback glyphs, which is acceptable.
