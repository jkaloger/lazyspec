---
title: Create seeds first lifecycle state and fix repairs invalid status
type: iteration
status: complete
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-221
---

## Objective

`create` seeds first lifecycle state; `fix` repairs out-of-lifecycle status.

## Satisfies

STORY-221 AC1–AC4. (fixes BUG-002)

## Context

- Observed live: BUG-001/BUG-002 born `status: draft`; bug lifecycle starts `reported`; `update --status` dead-ends ("no edge from draft, allowed targets: (none)"); `fix --dry-run` offers nothing
- Seed site: create path (find `draft` literal in create/template code); lifecycle: `TypeDef.lifecycle.states`
- Fix engine: `src/cli/fix/` + engine ops (field fixes)

## Tasks

1. Create: seed `lifecycle.states[0]` (default lifecycle still `draft` — no behaviour change).
2. `fix`: status not in type's lifecycle → field fix offering `states[0]`; `--dry-run` shows it.
3. Tests: bug-type create starts `reported`, `update --status triaged` ok; fix repairs planted bad status; default-lifecycle types unchanged. `cargo test`.

## Out of scope

Auto-migrating existing docs (fix is opt-in per doc).

## Verification

`cargo test`. Manual: `create bug x --json` → `"status": "reported"`.

