---
title: "CLI operation spinner via indicatif for create fetch prune"
type: iteration
status: draft
author: "Jack Kaloger"
date: 2026-07-20
tags: [cli]
related:
- implements: STORY-231
---

# Iteration: CLI operation spinner via indicatif for create fetch prune

## Objective

Animate the compact face during `create`/`fetch`/prune network waits via `indicatif` steady-tick; guard `--json`/non-TTY; draw to stderr.

## Satisfies

STORY-231 AC1 (spinner + finish glyph), AC2 (json/non-TTY unchanged bytes), AC3 (steady-tick motion), AC4 (stderr, clean teardown), colour AC (`FrameColour`→ANSI).

## Context

- Story + AC: STORY-231. Design: RFC-063 §Design (CLI). Point, don't restate.
- Dep (already present): `indicatif` `Cargo.toml:45` (unused today).
- Foundation (consume): `src/spinners.rs` (ITERATION-332) — compact frames.
- Create hook (TOUCH): no-op closures `src/main.rs:194,207`; plumbing `src/cli/create.rs:19,44`.
- Fetch hook (TOUCH): `src/cli/fetch.rs` `run` ~68-226.
- Prune hook (TOUCH): `src/cli/reservations.rs:109-192` (already prints `PruneProgress` lines).
- Progress enums (READ): `ReservationProgress`/`PruneProgress` `src/engine/reservation.rs:14-42`.
- Colour: `console::colors_enabled()` + existing `src/cli/style.rs` idiom.

## Tasks

1. CLI helper (new `src/cli/spinner.rs`): `fn op_spinner(msg, json: bool) -> Option<ProgressBar>` — `None` if `json || !stderr.is_terminal()`; else `ProgressBar` on stderr, `enable_steady_tick(~120ms)`, `ProgressStyle` `tick_strings` = face compact loading frames.
2. `FrameColour`→ANSI map (CLI side) via `console`/existing style helpers. No hex.
3. Wire create closures `main.rs:194,207`: `on_progress` maps `ReservationProgress`→state, `pb.set_message`; `finish_with_message` green ✔ / abandon red ✗.
4. `fetch.rs` + prune: wrap network span with `op_spinner`. Prune keeps its existing line output on the json/non-TTY path; spinner only when `Some`.
5. Ensure clear/finish leaves stdout unsmeared.

## Test Plan

- `create --json` and piped (non-TTY): assert no ESC (`\x1b`), stdout bytes == pre-change (parity fixture).
- Unit: `ReservationProgress`→`SpinnerState` map; `op_spinner` returns `None` under json/non-TTY.
- Manual TTY: run each op → face animates, finishes ✔/✗.

## Notes / conventions

- Dictum 2 (parity): json/non-TTY → zero animation, bytes unchanged.
- Dictum 3: CLI-only, engine untouched. Dictum 5: reuse `indicatif` idioms.
- Motion from steady-tick, not `on_progress` (fires only on state change; op blocking).

## Out of scope

- Init greeting (335). TUI (333/336). New spinner styles.
