---
title: Plugin install documentation
type: story
status: complete
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: RFC-051
---

## Story

As a new lazyspec user, I want a documented plugin install path in the README, so that I can choose the plugin channel without reverse-engineering the manifest.

Depends on S1 and S2 (the artifacts the docs describe must exist).

## Scope

- Add an "Install as a Claude Code plugin" section to the README, alongside the existing `lazyspec skills install` documentation.
- Document the two-command flow: `/plugin marketplace add jkaloger/lazyspec` then `/plugin install lazyspec@lazyspec`.
- State the prerequisite: the `lazyspec` binary must be on PATH; the convention hook is inert (silent noop) without it.
- Note coexistence: the plugin is an additional channel; `skills install` remains for AGENTS.md output and custom `[skills] entry` renaming.

## Out of scope

- Any CLI behavior change. README only.

## Acceptance criteria

- README has a plugin install section with the two `/plugin` commands.
- The binary-on-PATH prerequisite is stated.
- The relationship to `skills install` (additional channel, not replacement) is documented.
- CLAUDE.md guidance ("update the readme when the CLI interface changes") is satisfied even though no CLI signature changes — the install surface does.

