---
title: "Interactive add-type talking face greeting"
type: iteration
status: complete
author: "Jack Kaloger"
date: 2026-07-20
tags: [cli]
related:
- implements: STORY-231
---
<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Objective

Interactive `config add-type` greets with the talking full-box face, same as `init`.

## Satisfies

STORY-231 AC5 (talking greeting + `--json`/non-TTY guard), extended to the add-type wizard.

## Context

- Story + AC: STORY-231. `say` pattern already built (ITERATION-335).
- Reuse (no new code): `spinner::say`, `spinner::should_greet` (`src/cli/spinner.rs`); `console::colors_enabled` (see `init.rs:117`).
- Precedent (copy gate): `init.rs:117-118` — greet inside interactive flow, gated `should_greet(false, stdout.is_terminal(), colors_enabled())`.
- TOUCH: `src/main.rs` `Config`/`AddType`/`AddTypeInvocation::Prompt` branch — greet AFTER the `interactive` guard, BEFORE `run_add_type_interactive`.

## Changes

1. `main.rs` Prompt branch: after `interactive` confirmed, `if should_greet(json, stdout.is_terminal(), colors_enabled()) { say("...let's add a document type"); }`. Import `console::colors_enabled`.
2. Greeting stays at dispatch site (not inside `run_add_type_interactive`) → keeps that fn pure/`ScriptedPrompter`-driveable, no ESC in tests.

## Test Plan

- No new unit test: `should_greet` guard already covered (`spinner.rs`); greeting is terminal side-effect, not in the pure path.
- Manual TTY: `config add-type` (no args) talks then prompts.
- Guard: existing `ScriptedPrompter` add-type tests stay green (greeting not on their path).

## Out of scope

- Idle animation during prompts (deprioritised).
- `set-lifecycle` / `add-gate` greetings (no interactive path).

## Notes

Dictum 2: guard suppresses under `--json`/non-TTY/no-colour — no ESC bytes reach machine-readable output.
