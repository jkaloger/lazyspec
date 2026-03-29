---
title: "CLI Patterns"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [cli, patterns]
---

## Command Structure

- Every command gets its own module under `cli/` — `cli/show.rs`, `cli/validate.rs`, etc.
- Each module exports a `run()` function. If the command supports `--json`, it also exports `run_json()` or handles the flag internally
- Use clap derive macros for argument definitions. Keep the `Commands` enum in `cli.rs` as the single dispatch point
- `main.rs` does wiring only: parse args, load store, match command, call `run()`. No logic lives there

## JSON Output

- Every command that produces output must support `--json` for agent consumption. This is non-negotiable — agents are first-class consumers
- JSON output schemas should be consistent across commands — use the serialization patterns in `cli/json.rs`, don't invent per-command JSON shapes

## Output & Errors

- Output formatting goes through `cli/style.rs` — don't inline ANSI codes in command modules
- Errors surface to the user as human-readable messages. Don't print raw `Debug` output. `anyhow` context messages should be written for the person reading them

## ID Resolution

- Document ID arguments should go through the existing resolution/fuzzy matching — don't hand-roll ID parsing in individual commands
