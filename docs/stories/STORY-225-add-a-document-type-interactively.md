---
title: Add a document type interactively
type: story
status: complete
author: jkaloger
date: 2026-07-19
tags: []
related:
- implements: RFC-062
- blocks: STORY-226
- blocks: STORY-227
---> As a lazyspec user setting up a project, I want to run `config add-type` with no arguments on a terminal and answer a few prompts to add a document type, so that I don't have to memorise the flag grammar or the `.lazyspec.toml` schema.

This is the **walking skeleton** for the config wizard (RFC-062): the thinnest end-to-end path that proves the whole interactive stack — TTY detection, the CLI-layer prompt seam, and reuse of the existing `config add-type` engine writer — while stubbing every richer flow (attributes, custom lifecycle, gates, relations) for later stories.

## Scope

- `lazyspec config add-type` invoked on a TTY **with no positional arguments** starts an interactive prompt sequence for the core fields: `name`, `plural`, `dir` (defaulted to `docs/<plural>`), `prefix` (defaulted to uppercased name), plus `icon`, `store`, `numbering`, `singleton`, `authorship` as prompts with defaults.
- The wizard collects answers and calls the **same** engine writer the flag path already calls. No second config-writing path.
- The new type inherits the standard lifecycle preset; **customising lifecycle/attributes/gates/relations is out of scope** (STORY-226).
- Resolve RFC-062 open question: interactive prompting is **hand-rolled over stdin** unless a prompt crate is justified during implementation; TTY detection uses `std::io::IsTerminal`.

## Acceptance criteria

- **Given** a project with a config and a terminal, **when** I run `lazyspec config add-type` with no positional args, **then** I am prompted in order for name, plural, dir, prefix (dir/prefix pre-filled with defaults I can accept), and the type is appended to `.lazyspec.toml` identically to the equivalent flag invocation.
- **Given** I enter a type name or prefix that already exists, **when** the wizard validates it, **then** it reports the clash and re-prompts rather than aborting or writing a duplicate.
- **Given** stdin/stdout is **not** a terminal (piped, redirected, CI), **when** I run `config add-type` without the required positionals, **then** behaviour is unchanged from today (clap errors on the missing required args) — the wizard never starts.
- **Given** I pass `--json` or the four positional args, **when** the command runs, **then** it is fully non-interactive and byte-for-byte identical to today.
- **Given** the wizard completes, **then** the resulting config passes `lazyspec validate` (no malformed type written).

## Non-functional / constraints

- Prompting logic lives in the **CLI layer** (`src/cli/`), not the engine (Convention principle 3). A `Prompter` trait at the CLI seam allows a scripted fake in tests (principle 4); the real impl reads stdin.
- README: document that `config add-type` prompts interactively on a TTY with no args, and that flags/`--json`/non-TTY remain the canonical scriptable path.