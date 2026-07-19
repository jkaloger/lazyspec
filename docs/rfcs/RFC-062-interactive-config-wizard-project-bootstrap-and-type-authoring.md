---
title: 'Interactive config wizard: project bootstrap and type authoring'
type: rfc
status: accepted
author: jkaloger
date: 2026-07-19
tags: []
related:
- related-to: RFC-035
---## Summary

Add an **interactive config wizard** to lazyspec so a human can set up a project — or extend one — without hand-writing `.lazyspec.toml` or memorising the `config` flag grammar. Two entry points:

1. **Project bootstrap** — `lazyspec init` on a TTY walks the user through designing the full type DAG (types, stores, lifecycles, gates, relations) instead of writing a fixed starter config.
2. **Type authoring** — `lazyspec config add-type` on a TTY walks the user through one new type against an existing config.

The wizard is a **thin interactive front end** over the existing engine writers (`config_write`, `init::starter_config`). It collects the same inputs the current flags accept and produces the same `.lazyspec.toml`; it adds no second config-writing path. Interactivity is a convenience layer, never a requirement — every flow the wizard drives remains fully scriptable via flags, and non-TTY / `--json` invocation keeps today's non-interactive behaviour byte-for-byte.

## Motivation

Today configuring lazyspec means one of:

- `lazyspec init` — writes a fixed `starter_config()` (starter types + default rules). No choice; you take the shipped DAG or hand-edit TOML afterward.
- `lazyspec config add-type <NAME> <PLURAL> <DIR> <PREFIX> [--flags…]`, then `config set-lifecycle`, then `config add-gate` — three separate one-shot commands whose flag grammar (`--attribute NAME:KIND[:required][:VAL,…]`, store names, numbering strategies) you must already know.
- The `configure-type` **skill** — an agent interviews you and calls those same CLI writers. This is the *agent* path and works well, but it requires an agent in the loop.

There is no path for a **human without an agent** to design a DAG interactively. They either accept the starter config or learn the flag grammar and the TOML schema. RFC-035 already anticipated this — it named a `lazyspec init` / `lazyspec setup` "interactive wizard with flag overrides" for `git-ref` dirs — but that wizard was never generalised to the full type DAG. This RFC delivers it.

The wizard is the **human analogue of the `configure-type` skill**: same destination, same engine writers, driven by TTY prompts instead of an agent interview.

## Goals

- A human can bootstrap a complete, valid project config — full type DAG (types, lifecycles, gates, relations) — through guided prompts, no TOML knowledge required.
- A human can add one well-formed type (with lifecycle, gate, attributes, relations) to an existing project through guided prompts.
- The wizard writes through the **existing** engine writers; there is exactly one config-writing code path.
- Scriptability is preserved: any flow the wizard drives is reachable non-interactively via flags, and non-TTY / `--json` invocation is unchanged.
- Prompts validate inputs before they reach the engine (duplicate type names, unknown parent types, malformed attribute specs) and re-prompt rather than aborting.

## Non-goals

- Editing or migrating an **existing** type's lifecycle/gates/relations interactively (this RFC covers *add*, not *edit*). Out of scope; a follow-up.
- Replacing or deprecating the `configure-type` skill or the non-interactive `config` subcommands. The wizard sits alongside them.
- A TUI-based config editor. The wizard is line-oriented TTY prompting (stdin/stdout), not a ratatui screen.
- Remote-store auth flows (`setup github-issues`, `setup clickup`). Those stay in `setup`; the wizard may *offer* to run them but does not reimplement them.
- Reflowing existing documents when a lifecycle/type changes.

## Design

### Invocation model

Interactivity is inferred, never mandatory:

| Invocation | Behaviour |
|---|---|
| `lazyspec init` on a TTY, no flags | **Bootstrap wizard** |
| `lazyspec init` non-TTY (piped/CI) | Current behaviour: write `starter_config()` |
| `lazyspec init --non-interactive` | Force current behaviour on a TTY |
| `lazyspec config add-type` on a TTY, **no positional args** | **Add-type wizard** |
| `lazyspec config add-type <NAME> <PLURAL> <DIR> <PREFIX> …` | Current behaviour: one-shot from flags (never prompts) |
| any flow with `--json` | Non-interactive; `--json` implies non-TTY |

The rule: **presence of the disqualifying inputs (positional args, `--non-interactive`, non-TTY, `--json`) suppresses prompting.** A wizard only starts when it has nothing to work from and a human to ask. This keeps CI and scripted callers on exactly today's code path (Convention principle 2: agents consume the same interfaces as humans).

TTY detection uses `std::io::IsTerminal` on stdin and stdout (both must be terminals). No new dependency for detection.

### Layering (Convention principle 3)

```
CLI (src/cli/init.rs, src/cli/config.rs)
  └─ wizard prompt module (src/cli/wizard/…)      ← NEW: prompting + validation loop only
       └─ engine writers (config_write, init::starter_config)   ← unchanged
```

- The **wizard module lives in the CLI layer**, not the engine. It is I/O (reads stdin, writes prompts) and belongs where I/O formatting lives. The engine stays free of terminal assumptions (Convention principle 3: CLI/TUI depend on engine, never the reverse).
- The wizard **constructs the same value objects** the flag parsers construct (`TypeDef`, lifecycle edges, gate specs) and hands them to the **same** `config_write` / `starter_config` functions. No engine change is required for the wizard to exist; if a writer needs a new entry point, that is a small, separately-justified engine addition, not a parallel path.
- A thin **prompt trait** at the CLI seam (`Prompter`: `ask`, `select`, `confirm`, `ask_optional`) lets tests drive the wizard with a scripted fake instead of a real terminal (Convention principle 4: fakes only at trait seams). Real impl reads stdin; the fake replays a queue of answers.

### Bootstrap wizard flow (full DAG designer)

`lazyspec init` on a TTY:

1. **Project basics** — default author (prefaults to git `user.name`), naming pattern (default `{type}-{n:03}-{title}`).
2. **Types** — repeatedly: name, plural, dir (defaulted to `docs/<plural>`), prefix (defaulted to `NAME` upper), icon, store (`filesystem` / `github-issues` / `git-ref`, default filesystem), numbering, singleton?, authorship ceiling (`human`/`assisted`/`generated`). Each added type is echoed back; user adds another or finishes.
3. **Lifecycle per type** — offer a standard preset (`draft → review → accepted → in-progress → complete` + `rejected`/`superseded`) accepted by default, or design custom states + edges.
4. **Relations & DAG edges** — for each type, optionally pick a `parent_type` from already-defined types; define parent-child rules (`child needs parent`, severity warning/error) and `require_parent_status` gates.
5. **Relation vocabulary** — default to `starter_relationships()`; advanced users can add named relations with inverses.
6. **Review & confirm** — render the resulting DAG as a summary (types, edges, gates) and confirm before writing.
7. **Write** — call the existing `init` machinery (create dirs, templates, skeletons, gitignore, gh labels) but seeded from the *designed* `Config` rather than `starter_config()`. Offer "start from the starter DAG and tweak" as the first choice so the blank slate is opt-in.

Ordering matters: types before lifecycles before relations, because later steps reference earlier answers (a gate needs a parent type that already exists). The wizard resolves these dependencies in order and only offers valid targets at each step.

### Add-type wizard flow

`lazyspec config add-type` on a TTY with no positional args walks the single-type subset of the above: name/plural/dir/prefix/icon/store/numbering/singleton/authorship → attributes (`NAME:KIND[:required][:VAL,…]`, one at a time, validated) → optional lifecycle (else inherit the standard preset) → optional parent type + gate + relations. On confirm it calls the same functions `config add-type` + `config set-lifecycle` + `config add-gate` already call. It refuses (re-prompts) on a duplicate type name or unknown parent type.

### Validation & re-prompt

The wizard validates each answer against the engine's own rules *before* accepting it and re-prompts on failure rather than aborting a half-finished session: duplicate type/prefix, unknown parent type, malformed attribute spec, unknown store/numbering/authorship enum, a gate naming a non-existent status. Where the engine already exposes a validator, the wizard calls it; it does not reimplement validation.

### `--json` and machine consumption

`--json` implies non-interactive: it never prompts and preserves the current non-interactive output/behaviour. The wizard emits human-readable prose to stdout; it is not a `--json` producer. Agents continue to use the non-interactive flag interface and the `configure-type` skill (Convention principle 2 holds — the scriptable surface is unchanged and complete).

### Documentation

README updates: document TTY-triggered interactive mode for `init` and `config add-type`, the `--non-interactive` opt-out, and that flags/`--json`/non-TTY remain the canonical scriptable path. Note the wizard is the human counterpart to the `configure-type` skill.

## Alternatives considered

- **A dedicated wizard engine module with its own state machine.** Rejected: it would duplicate config-writing logic and risk a second, drifting path to `.lazyspec.toml`. The thin-front approach keeps one writer (Convention principle 6: indirection only for a second concrete use — there is only one config writer).
- **An explicit `init --wizard` / `config add-type --interactive` subcommand only.** More predictable, but less discoverable, and it leaves the bare commands as sharp edges (a fixed starter or a flag wall). Auto-on-TTY with a `--non-interactive` escape hatch gives discoverability without breaking scripts.
- **A ratatui TUI config editor.** Larger scope, and it fights the "line-oriented, pipe-friendly CLI" grain. Deferred; a TUI editor could layer on later over the same engine writers.
- **Do nothing; rely on the `configure-type` skill.** Leaves humans-without-agents with only the starter config or the flag grammar. The skill is the agent path; this is the human path.

## Open questions

- Should the bootstrap wizard's "start from starter and tweak" vs "blank slate" be a first-screen choice (recommended) or two separate entry commands?
- Should the add-type wizard offer to immediately create the type's first document, or stop at config? (Lean: stop at config; document creation is a separate verb.)
- Prompt/validation ergonomics for lifecycle edge entry — free-form `from→to` pairs vs picking from a rendered state list. Resolve during story breakdown.

## Impact on TUI / CLI / web (project constraint)

This is a **CLI-only interactive feature**; the TUI and web view have no interactive-config surface today and gain none here. The wizard writes standard `.lazyspec.toml` that the TUI and web view already consume unchanged. No TUI/web parity work is required — the parity constraint is satisfied because there is no new *viewing* capability, only a new *authoring* entry point that produces the same artifact all three layers already read. README (the CLI's docs) is the only doc surface to update.