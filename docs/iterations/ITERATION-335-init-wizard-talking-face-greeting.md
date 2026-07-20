---
title: "Init wizard talking face greeting"
type: iteration
status: draft
author: "Jack Kaloger"
date: 2026-07-20
tags: [cli]
related:
- implements: STORY-231
---

# Iteration: Init wizard talking face greeting

## Objective

Talking full-box face greets interactive `init` (eyes/mouth cycle per word, rest happy); suppressed under `--json`/non-TTY.

## Satisfies

STORY-231 AC5 (talking greeting + guard), colour AC (`FrameColour`→ANSI).

## Context

- Story + AC: STORY-231. Design: RFC-063 §Design (CLI greeting); Houston `say` pattern. Point, don't restate.
- Foundation (consume): `src/spinners.rs` (ITERATION-332) — `full` frames + face glyph sets.
- Init flow (TOUCH): `src/cli/init.rs` `run_init_interactive` ~100; `write_project` ~91.
- Guard: `console::colors_enabled()` / `is_terminal()` (existing `src/cli/style.rs`).
- ANSI colour map: reuse ITERATION-334 `FrameColour`→ANSI.

## Tasks

1. Hand-rolled `fn say(msg: &str)` in CLI (new; NOT indicatif): render full-box face, cycle eyes/mouth per word (`sleep` 75-200ms), redraw in place (crossterm cursor / reprint). Settle on the success/happy face.
2. Call at start of `run_init_interactive`; guard `stdout.is_terminal() && !json && colors_enabled()`.
3. `FrameColour`→ANSI reuse.

## Test Plan

- `init --json` and piped: assert no greeting emitted, no ESC.
- Manual TTY: greeting talks then rests.

## Notes / conventions

- Dictum 2: json/non-TTY → no output. Dictum 3: CLI-only.
- Greeting is bespoke animation (per-word), distinct from op steady-tick (334) — hand-rolled per RFC ADR.

## Out of scope

- Op spinners (334). TUI (333/336). Multi-message queue (single greeting only).
