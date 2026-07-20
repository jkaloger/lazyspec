---
title: Blank-by-default init wizard with --template starter
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-229
---

# Iteration: Blank-by-default init wizard with `--template starter`

## Objective

Interactive `init` wizard default first-screen -> BLANK DAG; opt into starter DAG via `lazyspec init --template starter`. Rename option `scratch`->`blank`. Non-interactive path byte-for-byte unchanged.

## Satisfies

STORY-229 AC "Blank-by-default + `--template starter`" (all 3 bullets). Resolves RFC-062 open question (first-screen choice vs separate command) -> flag-selected template.

## Context

- Parent story + full ACs: STORY-229 (`docs/stories/STORY-229-interactive-init-wizard-ux-polish.md`), scope bullet "Blank-by-default + `--template starter`".
- RFC-062 (implements link on story) -> one-config-writing-path guarantee + parity contract.
- Touch:
  - `src/cli.rs` — clap `Commands::Init` variant (line 76). Add `--template` arg beside `non_interactive`/`json`.
  - `src/main.rs` — dispatch block (lines 25-44). Destructure new `template` field; thread into interactive call.
  - `src/cli/init.rs` — `run_init_interactive` (line 100): first-screen select + branch. Signature grows a template param. `run()` (line 65) NOT touched.
  - `README.md` — init section (~line 144) + CLI table row (line 302).
- Do NOT touch (load-bearing): `run()` (init.rs:65) writes `starter_config()`; `write_project`; engine.

## Design decisions (bake into ACs)

- `--template` values: only `starter`. Enforce at clap parse (`value_parser` w/ possible value `starter`) -> unknown value errors before any IO. Default = absent = blank.
- `--template starter` PRE-SELECTS the starter designer -> wizard SKIPS first "Start from" screen, goes straight to `design_config_interactive(starter_config())`. No `--template` (or any non-`starter` future value handled by clap) -> first screen shown, default `blank`.
- First screen options `["blank", "starter"]`, default `"blank"`. Choice `"blank"` -> `design_config_from_scratch`; `"starter"` -> `design_config_interactive(starter_config())`.
- `--template` affects INTERACTIVE branch selection ONLY. Non-interactive/`--json`/non-TTY -> `run()` -> `starter_config()`, never consults `--template`. `--template` w/ `--non-interactive` = silently ignored (acceptable; `run()` already writes starter).

## Tasks

1. `src/cli.rs`: add `template: Option<String>` (or dedicated enum) to `Commands::Init` w/ `#[arg(long, value_parser = ["starter"])]` and doc comment (only `starter` supported; default blank). Order after `json`.
2. `src/main.rs`: extend the `Commands::Init { non_interactive, json }` destructure (line 25-28) to bind `template`; pass `template.as_deref()` into `run_init_interactive`. Non-interactive branch (`run(&cwd)`) unchanged.
3. `src/cli/init.rs` `run_init_interactive`: add param `template: Option<&str>`. If `template == Some("starter")` -> skip select, `config = design_config_interactive(starter_config(), prompter)?`. Else `select("Start from", &["blank", "starter"], "blank")`; `"starter"` -> starter designer, else (`"blank"`/default) -> `design_config_from_scratch(prompter)?`. Update the fn doc comment (lines 95-99).
4. `src/cli/init.rs` tests: fix callers of `run_init_interactive` (`init_bails_when_config_exists` ~line 554, `scratch_decline_writes_nothing` ~line 952) to pass the new `None` template arg. In `scratch_decline_writes_nothing` rename first-screen answer `"scratch"` (line 935) -> `"blank"`.
5. Add tests: (a) `--template starter` (call `run_init_interactive(root, &mut p, Some("starter"))`) routes to starter designer WITHOUT consuming a first-screen answer (script starts at author prompt) and produces starter types; (b) no template -> first-screen default `"blank"` routes to from-scratch designer (queue blank first answer, then a minimal scratch script).
6. `README.md`: init section (~144) — document blank-default first screen + `lazyspec init --template starter`; CLI table row (line 302) — replace "offering two paths -- tweak the starter DAG or design one from scratch" wording w/ blank-default + `--template starter`; note `--non-interactive`/`--json`/non-TTY still write starter config unchanged.

## Out of scope

- Pick-from-list `Prompter`/`multi_select` prompts -> STORY-229 slice 1.
- Wizard colours (`src/cli/style.rs`) -> STORY-229 slice 3.
- Wiring `--template` into non-interactive `run()` — explicitly excluded (parity).
- TUI/web: none (CLI-only; wizard emits standard `.lazyspec.toml`).

## Principles / conventions

- CLAUDE.md: update README when CLI interface changes (task 6).
- RFC-062 / STORY-229 Principle 2 (parity): non-interactive + `--json` byte-for-byte identical to today; `run()` untouched — THE load-bearing constraint. Guarded by existing `init_noninteractive_writes_starter` (init.rs:516) + `json_suppresses_interactive` (init.rs:526), both must pass unchanged.
- Principle 3 (layering): CLI layer only; engine untouched.
- Principle 4 (test seam): every flow scriptable via `ScriptedPrompter`; `--template starter` path scriptable via the flag.

## Verification

- `cargo test` — `init_noninteractive_writes_starter` + `json_suppresses_interactive` pass UNCHANGED (parity proof).
- New test: `--template starter` reaches starter designer without a queued first-screen answer.
- New test: default (no flag, blank first answer) -> from-scratch designer.
- `cargo run -- init --template bogus` errors at clap parse (unknown value).
- README init section + table row mention blank default and `--template starter`.

