---
title: Interactive project bootstrap from the starter config
type: story
status: in-progress
author: jkaloger
date: 2026-07-19
tags: []
related:
- implements: RFC-062
- blocks: STORY-228
---> As a person starting a new lazyspec project, I want to run `init` on a terminal and be walked through customising a config that starts from the shipped starter DAG — setting my author and naming and adding or dropping types — so that I get a config tailored to my project instead of the fixed starter, without editing TOML.

Adds an interactive front to `init`, reusing the add-type prompt flow (STORY-225/226). Seeds prompts from `starter_config()` so the user tweaks a working DAG rather than facing a blank slate (the blank-slate designer is STORY-228).

## Scope

- `lazyspec init` on a TTY with no flags starts the **bootstrap wizard** in "start from starter and tweak" mode.
- Prompts: default author (pre-filled from git `user.name`), naming pattern (default `{type}-{n:03}-{title}`), then per starter type an add/keep/drop choice, then "add another type" via the STORY-225/226 flow.
- On confirm, the **existing** `init` machinery (create dirs, templates, skeletons, gitignore, gh labels) runs — but seeded from the *designed* `Config`, not `starter_config()`.

## Acceptance criteria

- **Given** a directory with no `.lazyspec.toml` and a terminal, **when** I run `lazyspec init`, **then** I am prompted for author and naming (defaults offered), shown the starter types to keep/drop, and can add new types before writing.
- **Given** I accept every default, **then** the written config equals today's `starter_config()` output (parity escape hatch).
- **Given** stdin/stdout is not a terminal, or I pass `--non-interactive`, **when** I run `init`, **then** it writes `starter_config()` exactly as today — no prompts.
- **Given** `.lazyspec.toml` already exists, **then** `init` still bails as today (no interactive override).
- **Given** the wizard completes, **then** the scaffolded project passes `lazyspec validate`, and directories/templates/skeletons match the designed types.

## Non-functional / constraints

- Reuses the STORY-225/226 `Prompter` seam and add-type flow; `init`'s wizard adds no second config writer.
- README: document TTY-triggered interactive `init` and the `--non-interactive` opt-out.