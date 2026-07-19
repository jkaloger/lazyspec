---
title: 'Interactive config add-type: TTY wizard for core fields'
type: iteration
status: complete
author: jkaloger
date: 2026-07-19
tags: []
related:
- implements: STORY-225
---Implements STORY-225 (walking skeleton for the config wizard, RFC-062). One bounded slice: make `config add-type` prompt for its core fields on a TTY when given no positional args, reusing the existing `run_add_type` writer. Nothing here touches lifecycle, attributes, gates, or relations — those are STORY-226.

## Context

- `ConfigCommand::AddType` (`src/cli/config.rs:29`) declares 4 **required** positionals (`name`, `plural`, `dir`, `prefix`) + optional flags.
- Dispatch: `src/main.rs:612` destructures the variant → calls `run_add_type(&cwd, &fs, &name, …)` (`src/cli/config.rs:98`), which parses config, rejects duplicate name, pushes `TypeDef`, writes via `write_config_in_place`.
- No prompt/terminal crate in `Cargo.toml`. Use `std::io::IsTerminal` (std) + hand-rolled stdin reads. No new dependency.

## Approach

1. Make the 4 positionals `Option<String>` in the `AddType` clap variant. Trigger the wizard only when **all four are `None`**.
2. New CLI-layer prompt seam: `trait Prompter { fn ask(&mut self, label, default: Option<&str>) -> Result<String>; fn confirm(...); fn select(label, options, default) -> Result<String>; }`. Real impl reads stdin/writes stdout; a `ScriptedPrompter` (Vec queue) drives tests. Seam lives in `src/cli/` (Convention P3/P4).
3. New `run_add_type_interactive(root, fs, prompter) -> Result<()>`: prompt core fields (name, plural, dir[default `docs/<plural>`], prefix[default UPPER(name)], icon, store[default filesystem], numbering[default incremental], singleton[y/N], authorship[default assisted]) → re-prompt loop on duplicate name/prefix against loaded config → delegate to existing `run_add_type`. No second write path.
4. Dispatch (`src/main.rs`): if all 4 positionals `None` AND `stdin().is_terminal() && stdout().is_terminal()` AND not `--json` → `run_add_type_interactive`. Else if any positional present → existing `run_add_type` (unwrap the 4, error if partially supplied — clap-style required-together). Else (missing + non-TTY) → error as today.

## Task breakdown

- [ ] `AddType` positionals → `Option<String>`; add `#[arg(requires_all)]` or manual "all-or-none" check so partial positionals error.
- [ ] Define `Prompter` trait + `StdinPrompter` real impl + `ScriptedPrompter` test fake in `src/cli/` (new `wizard.rs` module).
- [ ] `run_add_type_interactive` in `src/cli/config.rs`: collect fields, defaults, duplicate re-prompt, delegate to `run_add_type`.
- [ ] Wire dispatch in `src/main.rs` with TTY + `--json` gating.
- [ ] README: `config add-type` prompts interactively on TTY with no args; flags/`--json`/non-TTY unchanged.

## Acceptance criteria

- **Given** a `ScriptedPrompter` queued with valid core-field answers, **when** `run_add_type_interactive` runs against a temp config, **then** the type is appended identically to the equivalent `run_add_type` flag call (assert TOML round-trip).
- **Given** the first queued name duplicates an existing type, **when** the wizard validates, **then** it re-prompts (consumes next answer) rather than erroring or writing a duplicate.
- **Given** `dir`/`prefix` answers are left blank, **then** defaults `docs/<plural>` and `UPPER(name)` are used.
- **Given** any positional arg is supplied, **then** dispatch takes the existing non-interactive `run_add_type` path (no prompting); **and** partial positionals error.
- **Given** `--json` or a non-terminal stdin/stdout, **then** no prompt is emitted and behaviour matches today.
- **Given** the wizard completes, **then** `lazyspec validate` passes on the resulting config.

## Test plan

- Unit tests in `src/cli/config.rs` / `wizard.rs` using `ScriptedPrompter` — no real TTY. Cover: happy path round-trip, duplicate-name re-prompt, blank-default fill, all-or-none positional guard.
- Reuse existing `run_add_type` round-trip test style (`add_type_round_trips`, `config.rs:498`).

## Out of scope

Lifecycle/attributes/gate/relation prompts (STORY-226); `init` wizard (STORY-227/228); prompt-crate adoption (hand-rolled stdin stands unless a later story justifies one).