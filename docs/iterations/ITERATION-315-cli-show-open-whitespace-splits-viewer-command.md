---
title: CLI show --open whitespace-splits viewer command
type: iteration
status: accepted
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-219
- related-to: BUG-005
---

## Objective

`show <id> --open` whitespace-splits viewer string (first token = binary, rest = args), matching TUI. Fixes BUG-005.

## Satisfies

BUG-005 (defect under STORY-219, AC1/AC4 viewer-command path).

## Context

- Layer: CLI only (src/cli).
- Bug: `spawn_open` (src/cli/show.rs:291) does `Command::new(&command).arg(&path)` on unsplit string → `viewer = "code -w"` fails "failed to launch viewer code -w".
- Reference impl: TUI splits on whitespace at src/tui/state/app.rs:312. Same treatment $EDITOR got in BUG-001 fix.
- Testability: prefer doing the split in `plan_open` (returns binary + args, unit-testable) rather than inside `spawn_open`.

## Tasks

1. Test-first: unit test on `plan_open` — `viewer = "code -w"` → binary `code`, args `["-w", <path>]`; single-token `viewer = "glow"` → binary `glow`, args `[<path>]`; empty/whitespace viewer → clear error, no panic.
2. Move whitespace split into `plan_open`: first token binary, remaining tokens args, doc path appended last.
3. `spawn_open` (show.rs:291) consumes plan: `Command::new(binary).args(args)`; drop unsplit `Command::new(&command)`.

## Out of scope

- Browser-URL open path (BUG-004 sibling handles JSON; URL resolution owned by STORY-219 base).
- Shell-quoting/escaping beyond whitespace split (parity w/ TUI split only).

## Principles/conventions

- CLAUDE.md: CLI/TUI parity — mirror TUI app.rs:312 split semantics exactly.

## Verification

`viewer = "code -w"` + `show <id> --open` launches `code -w <path>`. `cargo test`.
