---
title: 'Interactive add-type: attributes, lifecycle, gates and relations'
type: story
status: in-progress
author: jkaloger
date: 2026-07-19
tags: []
related:
- implements: RFC-062
---> As a lazyspec user adding a type, I want the interactive `config add-type` to also prompt me for attributes, a custom lifecycle, a parent type, a gate, and relations, so that I can author a fully-formed type — not just its core fields — without touching TOML.

Builds on the walking skeleton (STORY-225), which established the prompt seam and core-field flow. This layers the full single-type design surface on top, driving the same `config add-type`, `config set-lifecycle`, and `config add-gate` engine writers the flag paths already use.

## Scope

- Extends the STORY-225 prompt flow with optional sections: **attributes**, **lifecycle**, **parent type + gate**, **relations**.
- Attributes prompted one at a time in the engine's `NAME:KIND[:required][:VAL,…]` shape, validated per entry.
- Lifecycle: accept the standard preset by default, or design custom states and `from→to` edges.
- Parent type chosen only from types that already exist; optional `require_parent_status` gate; optional named relations.
- Calls the existing `set-lifecycle` / `add-gate` writers — no new engine config path.

## Acceptance criteria

- **Given** the add-type wizard is running, **when** I choose to add an attribute and enter `priority:enum:low,medium,high`, **then** it is validated and written; **and** a malformed spec (unknown kind, enum without values) is rejected with a re-prompt.
- **Given** I opt into a custom lifecycle, **when** I enter states and edges, **then** an edge naming a non-existent state is rejected and re-prompted; **and** if I decline, the type inherits the standard preset.
- **Given** I set a parent type, **when** I am offered choices, **then** only already-defined types are offered, and an unknown parent cannot be entered.
- **Given** I add a `require_parent_status` gate naming a status the parent lifecycle lacks, **then** it is rejected and re-prompted.
- **Given** I complete the flow, **then** the written config passes `lazyspec validate`, and the same result is reachable non-interactively via `add-type` + `set-lifecycle` + `add-gate` flags.

## Non-functional / constraints

- Reuses the STORY-225 `Prompter` seam; all validation calls existing engine validators, never a reimplementation (principle 6).
- README: document the extended interactive sections.