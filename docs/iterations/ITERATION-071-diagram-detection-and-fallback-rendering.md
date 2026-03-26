---
title: Diagram detection and fallback rendering
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: STORY-063
---




## Context

Foundation iteration for STORY-063. Covers terminal capability detection, diagram code block identification, and both fallback paths. The actual async rendering pipeline, inline image display, and caching are deferred to a follow-up iteration (AC3-AC6).

**ACs addressed:** AC1, AC2, AC7, AC8

## Changes

### Task 1: Terminal image protocol detection module

**ACs addressed:** AC1

**Files:**
- Create: `src/tui/terminal_caps.rs`
- Modify: `src/tui/mod.rs` (add module declaration)
- Modify: `src/tui/app.rs` (store detected protocol in App)
- Modify: `src/lib.rs` (if needed to re-export)

**What to implement:**

Define `TerminalImageProtocol` enum with variants `Sixel`, `KittyGraphics`, `None`.

Write a `detect()` function that checks environment variables at TUI startup:
- `TERM_PROGRAM=kitty` or `TERM=xterm-kitty` -> `KittyGraphics`
- `TERM_PROGRAM=iTerm.app` or `TERM_PROGRAM=WezTerm` -> `Sixel`
- Otherwise -> `None`

Add `terminal_image_protocol: TerminalImageProtocol` field to `App` struct, populated once in `App::new()`.

> [!NOTE]
> Keep the detection simple (env var checks only). Device attribute queries are fragile and can be added later if needed.

**How to verify:**
```
cargo test terminal_caps
```

### Task 2: Diagram code block detection

**ACs addressed:** AC2

**Files:**
- Create: `src/tui/diagram.rs`
- Modify: `src/tui/mod.rs` (add module declaration)

**What to implement:**

Write a `DiagramLanguage` enum: `D2`, `Mermaid`.

Write a `DiagramBlock` struct holding `language: DiagramLanguage`, `source: String`, `byte_range: Range<usize>` (position in the original markdown body).

Write `extract_diagram_blocks(body: &str) -> Vec<DiagramBlock>` that scans for fenced code blocks with language tag `d2` or `mermaid`. Use a simple line-by-line parser:
- Track opening `` ```d2 `` or `` ```mermaid `` fences
- Collect lines until the closing `` ``` ``
- Record the byte range for later substitution

This function is pure (no side effects) and operates on the markdown body string before it reaches `tui_markdown::from_str`.

**How to verify:**
```
cargo test diagram
```

### Task 3: CLI tool availability check

**ACs addressed:** AC7

**Files:**
- Modify: `src/tui/diagram.rs`

**What to implement:**

Write `is_tool_available(lang: DiagramLanguage) -> bool` that checks whether the corresponding CLI tool is on PATH:
- `D2` -> check for `d2` binary
- `Mermaid` -> check for `mmdc` binary

Use `std::process::Command::new("<tool>").arg("--version").output()` with a short timeout. Return `true` if the command succeeds, `false` otherwise.

Write `tool_name(lang: DiagramLanguage) -> &'static str` returning `"d2"` or `"mmdc"`.

**How to verify:**
```
cargo test diagram
```

### Task 4: Fallback rendering with hints in preview

**ACs addressed:** AC7, AC8

**Files:**
- Modify: `src/tui/ui.rs` (both `draw_preview_content` and `draw_fullscreen`)
- Modify: `src/tui/diagram.rs` (add hint message generation)

**What to implement:**

Write `fallback_hint(block: &DiagramBlock, tool_available: bool, protocol: TerminalImageProtocol) -> Option<String>` in `diagram.rs`:
- If tool is not available: return `Some("[d2: install d2 CLI for diagram rendering]")` (or mmdc equivalent)
- If protocol is `None`: return `Some("[diagram: terminal does not support inline images]")`
- If both are fine: return `None` (no fallback needed, future iteration will render)

In `ui.rs`, before passing the body to `tui_markdown::from_str`, call `extract_diagram_blocks()` and for each block where `fallback_hint()` returns `Some`, append the hint text after the code block in the body string. This is a string pre-processing step: the code block remains (syntax-highlighted by tui-markdown), and the hint appears as a styled line below it.

For this iteration, since the rendering pipeline doesn't exist yet, all diagram blocks will show a hint (either "install tool" or "no image support" or, if both tool and terminal are available, the block renders as plain code with no extra hint -- the rendering iteration will handle actual image display).

**How to verify:**
```
cargo test diagram
cargo test tui_diagram
```

## Test Plan

### T1: Protocol detection (AC1)

Unit tests in `tests/tui_diagram_test.rs`:

- `test_detect_kitty_protocol` -- set `TERM_PROGRAM=kitty`, assert `KittyGraphics`
- `test_detect_iterm_protocol` -- set `TERM_PROGRAM=iTerm.app`, assert `Sixel`
- `test_detect_wezterm_protocol` -- set `TERM_PROGRAM=WezTerm`, assert `Sixel`
- `test_detect_none_protocol` -- set `TERM_PROGRAM=SomeOther`, assert `None`

Tradeoff: these tests modify env vars, so they must run serially or use a function that accepts env values as parameters rather than reading `std::env` directly. Prefer the parameter approach for Isolated + Deterministic.

### T2: Diagram block extraction (AC2)

Unit tests in `tests/tui_diagram_test.rs`:

- `test_extract_d2_block` -- body with a `` ```d2 `` fence, assert one `DiagramBlock` with `D2` language and correct source/range
- `test_extract_mermaid_block` -- same for mermaid
- `test_extract_multiple_blocks` -- body with both d2 and mermaid, assert two blocks in order
- `test_extract_no_diagram_blocks` -- body with `` ```rust `` and `` ```json ``, assert empty vec
- `test_extract_nested_backticks` -- body with `` ```` `` (4-backtick fence), assert not falsely matched

These are pure function tests: fast, isolated, deterministic.

### T3: Tool availability (AC7)

Unit tests in `tests/tui_diagram_test.rs`:

- `test_tool_name_d2` -- assert `tool_name(D2) == "d2"`
- `test_tool_name_mermaid` -- assert `tool_name(Mermaid) == "mmdc"`

`is_tool_available` depends on system state, so test it indirectly: test the hint generation path instead (see T4).

### T4: Fallback hint generation (AC7, AC8)

Unit tests in `tests/tui_diagram_test.rs`:

- `test_hint_tool_missing` -- `fallback_hint(block, tool_available=false, protocol=KittyGraphics)` returns install hint
- `test_hint_no_image_support` -- `fallback_hint(block, tool_available=true, protocol=None)` returns terminal hint
- `test_hint_both_missing` -- `fallback_hint(block, tool_available=false, protocol=None)` returns install hint (tool missing takes priority)
- `test_hint_all_available` -- `fallback_hint(block, tool_available=true, protocol=KittyGraphics)` returns `None`

These are pure function tests: fast, isolated, deterministic, specific.

## Notes

- The `tui-markdown 0.3` crate handles syntax highlighting of code blocks. Diagram blocks with fallback will still render as syntax-highlighted code (the hint is appended, not substituted).
- The `crossterm 0.28` dependency already exists. Terminal detection uses env vars rather than crossterm's device attribute queries for simplicity and portability.
- The follow-up iteration (AC3-AC6) will add: async CLI invocation, PNG rendering, inline image display via the detected protocol, and content-hash caching.
