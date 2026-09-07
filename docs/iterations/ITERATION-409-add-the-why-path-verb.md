---
title: Add the why path verb
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-265
---

## Objective

`why <path>` lists the documents governing a file, human and `--json`.

## Satisfies

STORY-265 AC1, AC2, AC3.

## Context

- Story + ACs: STORY-265
- Exact JSON shape, and why this is a verb rather than a flag on `context` or `search`: RFC-068 §Design "Lookup", §Interfaces sample output, §Decisions 8
- Engine function: ITERATION-408.
- Touch: `src/cli/why.rs` (new), the `Commands` enum in `src/cli.rs` (single dispatch point, DICTUM-006), `src/main.rs` (wiring only), `src/cli/completions.rs` if the completion list is hand-maintained, `README.md` command reference.
- The argument is a file path, not a document id. Do not route it through `resolve_shorthand_or_path` (`src/cli/resolve.rs`); ID fuzzy-matching a path is wrong.
- Per result: id, type, title, status, `reviewed`, matching glob.

## Tasks

1. Test-first in `tests/integration/`: `why <path> --json` on a temp project with one pinned document returns one object carrying the six fields (AC1).
2. Test-first: two documents whose globs both match return two objects, each with its own glob (AC2).
3. Test-first: a path nothing matches returns `[]` with exit code zero (AC3).
4. Implement `cli/why.rs` with `run` / `run_json` per DICTUM-006; formatting through `cli/style.rs`, JSON through the `cli/json.rs` patterns.
5. Register in `cli.rs`, wire in `main.rs`, extend completions.
6. README: `why` in the command reference.

## Out of scope

- `show` output -- ITERATION-410.
- TUI and web lookup -- STORY-269.
- Any validation rule.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-006 for command module shape and `--json`.

## Verification

`cargo run --quiet -- why src/engine/store.rs --json` on this repo returns `[]` and exits 0 today; pin a scratch document at `src/engine/**` and it returns that document.
