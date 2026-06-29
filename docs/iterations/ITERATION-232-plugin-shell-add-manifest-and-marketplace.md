---
title: 'Plugin shell: add manifest and marketplace'
type: iteration
status: complete
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: STORY-172
---

## Goal

Plugin shell. Root manifest + same-repo marketplace. Skills load from root `skills/` via `/plugin install`.

## Tasks

- Add `.claude-plugin/plugin.json` at repo root. `name: "lazyspec"` (required). +`description`, +`version`, +`author`. No `commands`/`agents`. Loader auto-discovers `skills/` + `hooks/` relative to root.
- Add `.claude-plugin/marketplace.json` at root. `name: "lazyspec"`, `owner: { "name": "jkaloger" }`, `plugins: [...]`. Single entry → `source: "./"`, `strict: true`. (CORRECTION vs RFC draft: docs verify `strict: true` (default) → `plugin.json` is authority, auto-discovery of `skills/`+`hooks/` works. `strict: false` makes the marketplace entry the *entire* definition and a co-located `plugin.json` declaring components = conflict → load fails. RFC had the boolean inverted.)
- Both files coexist under `.claude-plugin/`. Distinct roles.
- Smoke: `/plugin marketplace add jkaloger/lazyspec` → `/plugin install lazyspec@lazyspec`. Confirm 10 skill dirs discovered (`lazy scaffold co-write generate advance execute review systematic-debugging configure-type create-audit`). Loose `skills/README.md` + `skills/MIGRATION-*` ignored (no `SKILL.md`). Live `/plugin` smoke requires the changes pushed to `jkaloger/lazyspec` (marketplace add fetches the remote); a user runs it in Claude Code.

## Acceptance criteria

- `marketplace add` registers marketplace `lazyspec`.
- `install lazyspec@lazyspec` succeeds.
- 10 skills resolve post-install from root `skills/`.
- Root `plugin.json` honoured (proves `source:"./"` + `strict:true`).

## Out of scope

- `hooks/hooks.json` → STORY-173.
- README → STORY-174.
- Scratch e2e → STORY-175.

