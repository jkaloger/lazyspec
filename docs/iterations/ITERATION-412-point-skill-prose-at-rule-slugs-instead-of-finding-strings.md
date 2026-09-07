---
title: Point skill prose at rule slugs instead of finding strings
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-266
---

## Objective

Prose that consumes `validate --json` selects on `rule` and reads fields, instead of grepping a sentence.

## Satisfies

STORY-266 AC5. Closes the story.

## Context

- Story + ACs: STORY-266
- The shape being consumed lands in ITERATION-411; RFC-068 §Design "Finding shape" has the example object.
- Touch: `skills/` -- the `/lazy` and `/execute` bodies named in AC5, plus anything else that pipes `validate --json`. `README.md` too, if it documents the output shape.
- Skills ship from this repo (`src/engine/skills.rs`, `src/cli/skills.rs`). Change them at their source, never in an installed copy.
- Sandbox: `Bash` writes under `skills/` are denied here. Use the `Write` / `Edit` tools.
- This is prose only. No Rust changes; if a skill instruction cannot be expressed against the new shape, that is a finding for the report, not a reason to widen the shape.

## Tasks

1. Grep every `validate --json` consumer in prose across `skills/`, `README.md`, `docs/`. List them before editing.
2. Rewrite each string-parsing instruction to `jq 'select(.rule == "...")'` plus field reads.
3. README: if it shows the `validate --json` shape, update the example to the object form and note the break.

## Out of scope

- Engine or CLI changes. The shape already landed.
- Skills that do not consume findings.
- Documenting the `governs-*` slugs -- they do not exist yet.

## Principles/conventions

`cargo run --quiet -- convention`. Convention principle 2: agents consume the same interfaces humans do, so the prose and the JSON must agree.

## Verification

No prose under `skills/` or in `README.md` matches on a finding message. Every `validate --json` example in the repo runs against current output and returns what the surrounding text claims.
