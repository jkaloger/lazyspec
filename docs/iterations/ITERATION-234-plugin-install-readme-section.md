---
title: Plugin install README section
type: iteration
status: complete
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: STORY-174
---

## Goal

README plugin install path. Alongside `skills install`.

## Tasks

- Add README section "Install as a Claude Code plugin".
- Document 2-cmd flow: `/plugin marketplace add jkaloger/lazyspec` → `/plugin install lazyspec@lazyspec`.
- State prereq: `lazyspec` binary on PATH. Hook inert (silent noop) without it.
- State coexistence: plugin = additional channel. `skills install` stays for AGENTS.md target + custom `[skills] entry` rename.

## Acceptance criteria

- README has plugin section w/ both `/plugin` cmds.
- Binary-on-PATH prereq stated.
- Relationship to `skills install` (additional, not replacement) documented.

## Out of scope

- CLI behavior change. README only.

## Depends

STORY-172, STORY-173 (artifacts must exist to document).

