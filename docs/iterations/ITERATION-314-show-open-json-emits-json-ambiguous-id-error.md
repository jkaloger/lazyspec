---
title: show --open --json emits JSON ambiguous_id error
type: iteration
status: complete
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-219
- related-to: BUG-004
---

## Objective

`show <id> --open --json` on ambiguous shorthand emits same JSON error object as plain `show --json`, not human stderr. Fixes BUG-004.

## Satisfies

BUG-004 (defect under STORY-219, AC2 CLI parity path).

## Context

- Layer: CLI only (src/cli).
- Bug: `run_open` (src/cli/show.rs:214) handles `ResolveError::Ambiguous` via `eprintln!` BEFORE consulting `json` flag → exit 0, no JSON. `run_json` (src/cli/show.rs:182) has correct branch emitting `{"error":"ambiguous_id","ambiguous_matches":[...]}`.
- Target shape (copy exactly from run_json:182): `{"error":"ambiguous_id","ambiguous_matches":[...]}`.

## Tasks

1. Test-first: CLI test — ambiguous shorthand + `--open --json` → stdout parses as `{"error":"ambiguous_id","ambiguous_matches":[...]}`, matches run_json output; no human stderr text.
2. In `run_open` (show.rs:214): when `json` set + `ResolveError::Ambiguous`, print same JSON error object as run_json:182 and return (factor shared emit helper if trivial).
3. Preserve non-json `--open` human stderr behaviour unchanged.

## Out of scope

- BUG-005 viewer whitespace-split (sibling iteration).
- Other ResolveError variants beyond Ambiguous (unless run_json diverges — match it).

## Principles/conventions

- CLAUDE.md: machine-readable JSON for agent-driven commands. CLI depends on engine, never TUI.

## Verification

Two colliding shorthands; `show <ambig> --open --json` stdout == `show <ambig> --json` stdout. `cargo test`.
