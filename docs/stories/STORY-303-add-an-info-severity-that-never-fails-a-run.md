---
title: Add an info severity that never fails a run
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-067
- related-to: RFC-075
---

## Value

As a pack author, I want an `info` finding severity, so hooks and edges can surface advice that never fails a run.

## Acceptance Criteria

- AC1: `Severity` gains `Info` (`src/engine/config.rs:14`). `[[edges]] required` and hook findings accept `"info"`.
- AC2: An `info` finding never changes the exit code of `validate` or blocks a transition (`has_error` in `src/engine/pre_transition.rs`).
- AC3: Text output hides `info` unless `--warnings` is passed. `--json` always includes them, with `severity: "info"`.
- AC4: The TUI shows `info` findings in a distinct style; the settings editor offers it for edges.
- AC5: `config` edge commands (`src/cli/config.rs:1125`) accept `info`.

## Scope

### In Scope

- `Severity` and every exhaustive match on it: config, validation, hooks, `status`, `validate`, TUI settings (`src/tui/state/app.rs:166`).
- README hook protocol docs list the new value.

### Out of Scope

- A separate flag to show info only.
- Web view: removed by ADR-037.

## Notes

`--warnings` as the reveal flag keeps one verbosity axis. A dedicated `--info` is rejected until a second need appears.
