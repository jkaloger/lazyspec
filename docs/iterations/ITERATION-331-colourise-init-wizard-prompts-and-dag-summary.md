---
title: Colourise init wizard prompts and DAG summary
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-229
---

# Iteration: Colourise init wizard prompts and DAG summary

## Objective

Wire `src/cli/style.rs` colour helpers into interactive `init` wizard prompts + `render_dag_summary`; plain-text fallback when colours off. Presentation only.

## Satisfies

STORY-229 "Wizard colours" ACs (both):
- colour-capable TTY -> prompts + DAG summary use `src/cli/style.rs` helpers.
- colours off (`colors_enabled()` false: piped, `NO_COLOR`, non-TTY, `--json`) -> plain text, zero ANSI escapes.

Cross-cutting: layering AC (all in `src/cli/`, engine untouched); one config-writing path unchanged.

## Context

- Story + AC text: STORY-229 (`docs/stories/STORY-229-interactive-init-wizard-ux-polish.md`), "Wizard colours" block.
- Parent spec: RFC-062 (one-config-writing-path + parity contract). Point, don't restate.
- Colour helpers (READ, reuse): `src/cli/style.rs` -> `bold(text)`, `dim(text)`, `type_header(&DocType)`, `separator()`, `error_prefix()`, `warning_prefix()`, `styled_status(...)`. All route through `console::colors_enabled()` already (see `separator`/`error_prefix`/`warning_prefix` bodies -> plain branch when false). Pattern to copy for new helpers: `if colors_enabled() { styled } else { plain }`.
- Prompt strings (TOUCH): `src/cli/wizard.rs` -> `StdinPrompter::ask`/`confirm`/`select` build plain `format!` prompts (~lines 54-86). Colour goes on label + default/hint here. `ScriptedPrompter` emits no output -> leave untouched.
- Init flow (TOUCH): `src/cli/init.rs`:
  - `render_dag_summary` (~242): plain `writeln!` builder returning `String`. Colourise here.
  - `run_init_interactive` (~100): "Start from" flow -> section transitions.
  - `write_project` (~91): success line `Initialized lazyspec in {}`.
  - inline msgs: `"discarded; starting over"` (~163, ~234), `"at least one type is required"` (~214).

## Tasks

1. Add to `src/cli/style.rs` (only if a plain `bold`/`dim` compose won't do): small `colors_enabled()`-aware wizard helpers, each with a plain fallback branch:
   - `section_header(text: &str) -> String` — styled section divider for flow transitions (reuse `separator()` idiom; plain = the text as-is or `--- text ---`).
   - `prompt_label(label: &str) -> String` — bold label for `StdinPrompter` (plain = `label` verbatim).
   - `success_line(text: &str) -> String` — success cue (e.g. green check prefix, mirror `error_prefix`/`warning_prefix` shape; plain = `text`).
   Keep signatures generic (`&str`), no engine imports beyond existing.
2. `src/cli/wizard.rs` `StdinPrompter` only: route the label (and `[default]`/`[Y/n]`/`(opts) [default]`) through the style helpers in `ask`/`confirm`/`select`. Style the label bold, defaults/hints `dim`. Preserve exact trailing `: ` and bracket layout so scripted parity + read parsing unaffected. `ScriptedPrompter` unchanged.
3. `src/cli/init.rs`:
   - `render_dag_summary`: type names `bold`, edges/gates/dirs/prefixes `dim`, the `Types:`/`Parent-child rules:`/`Relation vocabulary:` headers via `section_header` (or `bold`). Keep the returned-`String` shape and field order identical.
   - `run_init_interactive`: wrap "Start from" transition with a `section_header`.
   - `write_project` success line -> `success_line`.
   - warn/error inline msgs -> prepend `warning_prefix()` / `error_prefix()` (`at least one type is required` = warning; `discarded; starting over` = warning/dim).
4. Tests (in `src/cli/init.rs` `#[cfg(test)]` and/or `src/cli/style.rs`): add a plain-parity test asserting no ESC (`\x1b`) in rendered output when colours disabled, and that content is byte-identical to today's plain form. See Verification for the `colors_enabled()` toggle mechanism.

## Out of scope

- Slice 1 (pick-from-list / `multi_select` on `Prompter`): deferred. **Dependency:** this slice edits the same `StdinPrompter` prompt-building code slice 1 touches — land slice 1 first to avoid churn.
- Slice 2 (blank-by-default first screen + `--template starter`): deferred.
- Non-interactive `run()` / `starter_config()` write path: unchanged (parity).
- Any engine change, TUI, web view.

## Principles / conventions

- CLAUDE.md: dogfood via `cargo run`; update README only if CLI surface changes (this slice adds none — colour is automatic).
- RFC-062 Principle 2 (parity): `--json`/non-TTY produce zero colour; output content byte-for-byte unchanged. Principle 3 (layering): all edits under `src/cli/`; engine untouched. Principle 4 (test seam): colour is presentational, not a `Prompter` behaviour change — `ScriptedPrompter` produces no output and stays as-is.
- Global: no comments restating code; keep new helpers terse.

## Verification

- Assert plain parity with colours forced off. `console::colors_enabled()` reads global state; set it deterministically in tests via `console::set_colors_enabled(false)` before rendering, and assert `!summary.contains('\u{1b}')` plus equality against the known plain string. (If a global toggle proves flaky across tests, gate the assertion on `colors_enabled()` already being false under `cargo test`'s non-TTY stdout — the default — and additionally add an explicit `set_colors_enabled(true)` case asserting an ESC IS present, then reset.)
- Existing `scratch_summary_lists_dag` (init.rs ~856) must still pass: its `summary.contains("rfc")`, `"draft -> accepted"`, `"story -> rfc"`, `"parent status = accepted"`, `"implements"` substring assertions must survive colourisation — so do NOT inject ANSI mid-substring in the plain path, and ensure the styled path still contains those raw substrings (ANSI wraps whole tokens, not split them). Run `cargo test` (or `cargo run` build if warnings block) and confirm the full init/style suites green.

