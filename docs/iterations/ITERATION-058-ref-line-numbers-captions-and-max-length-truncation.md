---
title: Ref line numbers, captions, and max-length truncation
type: iteration
status: draft
author: agent
date: 2026-03-12
tags: []
related:
- implements: docs/stories/STORY-058-ref-expansion-hardening-and-performance.md
---


## Context

RFC-019 introduced `@ref` directives for embedding code into spec documents. The current implementation supports `@ref path`, `@ref path#Symbol`, and `@ref path#Symbol@sha`. This iteration adds three enhancements: line number references, metadata captions on expanded blocks, and configurable truncation for long snippets.

**Story ACs addressed:**
- None of STORY-058's ACs are directly about these features; this iteration extends the ref system with new capabilities that emerged during usage. It links to STORY-058 as the parent hardening/performance story.

## Changes

### Task 1: Line number references in RefExpander

**Files:**
- Modify: `src/engine/refs.rs`

**What to implement:**

Update `REF_PATTERN` to continue capturing the `#<value>` group as-is. In `resolve_ref`, after extracting the `symbol` capture group, check whether it is purely numeric (all ASCII digits). If so, treat it as a 1-based line number instead of a symbol name.

When the `#` value is numeric:
1. Parse it as `usize`
2. After fetching file content via `git show`, split into lines
3. Return lines starting from that line number (0-indexed: `line_num - 1`), up to `max_lines` (see Task 3)
4. If the line number exceeds the file length, return an unresolved warning

The regex itself does not need to change since numeric values already match `[^@\s]+`. The branching happens in `resolve_ref`.

**How to verify:**
```
cargo test --test expand_refs_test
```

### Task 2: Markdown caption on expanded code blocks

**Files:**
- Modify: `src/engine/refs.rs`

**What to implement:**

In `resolve_ref`, after resolving the content and before building the fenced code block string, generate a caption line and prepend it.

Caption format:
- With explicit SHA: `**src/foo.rs** @ \`abc1234\` (L42)` or `**src/foo.rs** @ \`abc1234\` (SymbolName)`
- Without SHA (HEAD): resolve the current HEAD short SHA by running `git rev-parse --short HEAD` in `self.root`, then format the same way

The caption goes on its own line immediately before the opening code fence. For symbol refs, use the symbol name in parens. For line number refs, use `L<n>`. For whole-file refs (no `#`), omit the parens entirely.

Unresolved refs keep their existing `> [unresolved: ...]` format with no caption.

**How to verify:**
```
cargo test --test expand_refs_test
```

### Task 3: Configurable max_lines with truncation

**Files:**
- Modify: `src/engine/refs.rs`

**What to implement:**

Add a `max_lines: usize` field to `RefExpander`, defaulting to 25. Update `RefExpander::new` to accept this parameter (or add a builder method / second constructor).

In `resolve_ref`, after extracting content (whether from symbol extraction, line number extraction, or whole-file), count the lines. If the line count exceeds `max_lines`:
1. Take only the first `max_lines` lines
2. Append a language-appropriate comment: `// ... (N more lines)` for C-family/Rust/Go/Java, `# ... (N more lines)` for Python/YAML/TOML, `<!-- ... (N more lines) -->` for markdown

This applies uniformly to symbol refs, line number refs, and whole-file refs.

**How to verify:**
```
cargo test --test expand_refs_test
```

### Task 4: Wire --max-ref-lines through CLI

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/cli/show.rs`
- Modify: `src/engine/store.rs`
- Modify: `src/tui/app.rs` (if TUI calls `get_body_expanded`)

**What to implement:**

Add `--max-ref-lines <N>` optional arg to the `Show` variant in `cli/mod.rs` (clap `#[arg(long, default_value_t = 25)]`). Thread it through `show::run` and `show::run_json` to `Store::get_body_expanded`. Update `get_body_expanded` to accept `max_lines: usize` and pass it to `RefExpander::new`.

For the TUI path, use the default (25) unless a config mechanism already exists.

**How to verify:**
```
cargo run -- show -e RFC-019 --max-ref-lines 10
cargo run -- show -e RFC-019
```

### Task 5: Tests

**Files:**
- Modify: `tests/expand_refs_test.rs`

**What to implement:**

Add test cases for each new feature. See Test Plan below.

**How to verify:**
```
cargo test --test expand_refs_test
```

### Task 6: Update README

**Files:**
- Modify: `README.md`

**What to implement:**

Update the `@ref` syntax section to document:
- Line number syntax: `@ref path#123`, `@ref path#123@sha`
- The caption that appears on expanded blocks
- The `--max-ref-lines` flag
- Default truncation behavior (25 lines)

**How to verify:**

Read the README and confirm it matches the implementation.

## Test Plan

### Line number refs

| Test | What it verifies |
|------|-----------------|
| `test_line_number_ref_extracts_from_line` | `@ref Cargo.toml#1` expands to content starting at line 1, code fence present |
| `test_line_number_ref_with_sha` | `@ref Cargo.toml#1@<valid-sha>` resolves against that commit |
| `test_line_number_ref_out_of_bounds` | `@ref Cargo.toml#99999` produces an unresolved warning |
| `test_line_number_vs_symbol_disambiguation` | `@ref src/engine/refs.rs#RefExpander` still works as a symbol ref (not treated as line number) |

### Captions

| Test | What it verifies |
|------|-----------------|
| `test_expanded_ref_has_caption` | Expanded output contains `**Cargo.toml**` and a short SHA before the code fence |
| `test_caption_includes_symbol_name` | Symbol ref caption contains `(SymbolName)` |
| `test_caption_includes_line_number` | Line number ref caption contains `(L42)` |
| `test_caption_with_explicit_sha` | Explicit SHA appears in caption as provided (not re-resolved) |
| `test_unresolved_ref_no_caption` | Unresolved refs have no caption line |

### Truncation

| Test | What it verifies |
|------|-----------------|
| `test_max_lines_truncates_long_content` | Whole-file ref with `max_lines=5` produces exactly 5 content lines plus a truncation comment |
| `test_max_lines_no_truncation_when_short` | Content under the limit is not truncated |
| `test_truncation_comment_style_rust` | `.rs` file gets `// ... (N more lines)` |
| `test_truncation_comment_style_python` | `.py` file gets `# ... (N more lines)` |

All tests are unit-level, deterministic, and fast. They use the local repo's committed files as fixtures (same pattern as existing tests in `expand_refs_test.rs`).

## Notes

- The `#` capture group already matches numeric values, so no regex change is needed. The branching is purely in `resolve_ref`.
- `git rev-parse --short HEAD` adds one subprocess call per expansion pass (not per ref). Consider caching it at the start of `expand()`.
- The truncation comment style could be a simple match on the language tag already computed by `language_from_extension`.
