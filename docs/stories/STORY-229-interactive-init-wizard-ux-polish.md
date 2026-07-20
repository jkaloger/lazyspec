---
title: Interactive init wizard UX polish
type: story
status: in-progress
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: RFC-062
---

> As a lazyspec user bootstrapping a project with the interactive `init` wizard, I want to pick options from a list instead of retyping them, start from a blank DAG by default (opting into the starter with a flag), and read colour-cued prompts, so that the wizard is faster, less error-prone, and easier to follow — without changing anything scripts and CI already rely on.

This story polishes the **already-shipped** interactive `init` wizard (delivered by STORY-225 through STORY-228). It carries no new engine capability: it improves how the existing CLI-layer wizard *presents* choices, what it defaults to, and how it reads. It is deliberately sliced into three cohesive, independently observable UX improvements (which will become three iterations). All three keep RFC-062's one-config-writing-path guarantee and the non-interactive/`--json` byte-for-byte contract.

## Scope

- **Pick-from-list prompts.** Extend the `Prompter` trait (`src/cli/wizard.rs`) so choosing an option is a real chooser rather than typing the option string verbatim, and add a `multi_select` capability for choosing several options at once. Replace the verbatim `select` callsites in the type/rule collectors (store, numbering, authorship, lifecycle states, severity in `collect_type_interactive` and parent-child rule collection).
- **Blank-by-default + `--template starter`.** Change the interactive wizard's "Start from" first screen (`src/cli/init.rs` `run_init_interactive`) to default to a **blank** DAG, rename the `scratch` option to `blank`, and add a `lazyspec init --template starter` flag to opt into the starter DAG. This resolves RFC-062's open question (first-screen choice vs. separate command) in favour of a flag-selected template.
- **Wizard colours.** Wire the existing `src/cli/style.rs` helpers (`bold`, `dim`, section headers, success/error prefixes, `colors_enabled()`) into the wizard prompts and the `render_dag_summary` output.

### Out of scope

- The non-interactive `run()` path (`src/cli/init.rs`, which writes `starter_config()`): **unchanged**. Scripts and CI keep today's behaviour exactly.
- Any engine change; any new document type or rule semantics.
- TUI and web view: the wizard writes a standard `.lazyspec.toml` that all layers already read, so no parity work is required (CLI-only feature).

## Acceptance criteria

### Multi-select / pick-from-list prompts

- **Given** the wizard reaches a single-choice prompt (store, numbering, authorship, severity), **when** it renders on a TTY, **then** I choose from a presented list rather than typing the option text verbatim, and choosing an out-of-list value is not silently accepted.
- **Given** a prompt that legitimately accepts several values (e.g. lifecycle states), **when** I use the new `multi_select` capability, **then** I can select more than one option in a single prompt and all selections are captured.
- **Given** the test suite, **when** it drives the wizard through `ScriptedPrompter`, **then** every new/changed prompt (including `multi_select`) remains fully scriptable via queued answers with no real stdin (Convention principle 4: fakes only at the `Prompter` trait seam).

### Blank-by-default + `--template starter`

- **Given** I run `lazyspec init` on a TTY with no template flag, **when** the wizard shows its first screen, **then** the default is **blank** (the from-scratch designer) and the option is labelled `blank`, not `scratch`.
- **Given** I run `lazyspec init --template starter` on a TTY, **when** the wizard starts, **then** it seeds the starter DAG (today's `starter_config()` designer path).
- **Given** I run `init` non-interactively or with `--json` (or on a non-TTY), **when** it writes the config, **then** the output is **byte-for-byte identical to today** — the `run()` path still writes `starter_config()` and never consults `--template` for interactive branching (Convention principle 2).

### Wizard colours

- **Given** a colour-capable TTY, **when** the wizard renders prompts and the DAG summary (`render_dag_summary`), **then** labels/headers/success cues use the `src/cli/style.rs` helpers.
- **Given** a non-TTY or colours disabled (`colors_enabled()` false, e.g. piped, `NO_COLOR`, `--json`), **when** the same output is produced, **then** it degrades to plain text with no ANSI escape codes.

### Cross-cutting

- **Given** any of the three flows completes and scaffolds a project, **when** I run `lazyspec validate`, **then** the resulting config validates clean.
- **Given** the change set, **when** reviewed, **then** all prompting/colour/template-branching logic lives in the CLI layer (`src/cli/`) with the engine untouched (Convention principle 3), and there remains exactly one config-writing path shared by interactive and non-interactive `init` (RFC-062).

## Non-functional / constraints

- **Principle 2 (parity):** non-interactive + `--json` paths are byte-for-byte unchanged; every wizard flow remains scriptable via flags (`--template`, positional/flag answers) and via `ScriptedPrompter` in tests.
- **Principle 3 (layering):** prompting, colour, and template selection stay in `src/cli/`; the engine writers are reused, not modified.
- **Principle 4 (test seam):** the `Prompter` trait is the only place a fake is introduced; the real impl reads stdin, `ScriptedPrompter` drives tests, and it must stay scriptable through the new `multi_select` addition.
- **Project constraint:** CLI-only — no TUI/web parity work (the wizard emits a standard `.lazyspec.toml` all layers already read). README is the only documentation surface and must be updated for the blank-default first screen and the new `--template starter` flag.

