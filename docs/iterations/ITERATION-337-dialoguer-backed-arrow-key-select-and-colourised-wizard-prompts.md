---
title: Dialoguer-backed arrow-key select and colourised wizard prompts
type: iteration
status: complete
author: Jack Kaloger
date: 2026-07-20
tags:
- cli
- wizard
- ux
related:
- implements: STORY-229
---
<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Objective

Init wizard prompts navigate by ↑/↓ (Enter to pick, Space to toggle multiselect) and render coloured, replacing numbered typing + bold/dim-only styling.

## Satisfies

STORY-229 (wizard UX polish). Extends the colour work from ITERATION-331 — fixes root cause: prompts today use only `bold`/`dim` (no hue), `multi_select` unstyled (`src/cli/wizard.rs:130-142`).

## Context

- Story: STORY-229.
- Seam: `Prompter` trait — `src/cli/wizard.rs:8` (`ask`/`confirm`/`select`/`multi_select`). Real impl `StdinPrompter` (`:38-167`); test fake `ScriptedPrompter` (`:206`).
- Wizard is TTY-gated: `src/main.rs:668-669`. Real prompter only runs on a TTY.
- Colour crate `console` (`Cargo.toml:35`) already in tree via `indicatif`. `dialoguer` builds on the same `console 0.15` → shares colour state, no new colour stack.
- Callsites: `src/cli/init.rs`, `src/cli/config.rs` (add-type). Style helpers `src/cli/style.rs`.

## Changes

1. Add `dialoguer` dep (`cargo add dialoguer`, pin to the `console 0.15`-compatible line, e.g. `0.11`). No default `password`/`fuzzy` features needed.
2. Reimplement `StdinPrompter` real path over dialoguer + `ColorfulTheme`, drawing to `console::Term`:
   - `ask` → `Input`, `confirm` → `Confirm`, `select` → `Select`, `multi_select` → `MultiSelect` (empty-`options` freeform branch stays an `Input` split on `,`, per current contract wizard.rs:129-135).
   - Preserve the trait contract exactly: blank→default, 1-based/exact resolution now handled by nav (no typed numbers), de-dup + input-order for multiselect.
   - Drop the `R: BufRead, W: Write` generics on the real impl (dialoguer owns the terminal). `ScriptedPrompter` unchanged — remains the callsite test seam.
3. Delete the now-redundant `StdinPrompter` Cursor-based re-ask/validation unit tests (wizard.rs:284-345) — dialoguer owns navigation/validation; don't re-test the library. Keep `ScriptedPrompter` tests.
4. Update README wizard section (project rule: CLI-interface change → README): numbered prompts → arrow-key.

## Test Plan

- Callsite tests (`init.rs`, `config.rs`) via `ScriptedPrompter` stay green — prove wizard logic unchanged.
- Manual (TTY): `cargo run -- init` → blank-slate designer; select prompts move on ↑/↓, multiselect toggles on Space, selection line coloured.
- `--json` and non-TTY: wizard skipped (main.rs gate) → output bytes unchanged; assert existing non-interactive init tests unaffected.

## Out of scope

- TUI (no dialoguer in TUI layer — dictum 3).
- Non-wizard CLI prompts; mascot RFC-063 spinners.
- Changing `ScriptedPrompter` semantics or the `Prompter` trait signatures.

## Notes

- Key decision: real prompter becomes TTY-only (acceptable — wizard already gated main.rs:668). Dead non-TTY line path removed; trait seam + `ScriptedPrompter` keep tests hermetic (dictum 4).
- `ColorfulTheme` uses `console`'s colour state → honours `NO_COLOR`/`CLICOLOR` automatically; no separate gate.
- Principles: `.claude` conventions (TDD, trait seams for I/O — dictum 4; ecosystem norm over hand-roll — dictum 5).
