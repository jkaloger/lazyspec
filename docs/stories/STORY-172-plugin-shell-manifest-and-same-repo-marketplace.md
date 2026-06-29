---
title: 'Plugin shell: manifest and same-repo marketplace'
type: story
status: draft
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: RFC-051
- blocks: STORY-173
- blocks: STORY-174
- blocks: STORY-175
---

## Story

As a lazyspec user on Claude Code, I want to install the integration through a plugin marketplace, so that the skills load from one `/plugin install` instead of `lazyspec skills install` plus hand-edited settings.

This story delivers the plugin *shell*: the manifest and the same-repo marketplace that make the plugin discoverable and installable. The convention hook (S2), docs (S3), and end-to-end validation (S4) build on top.

## Scope

- `.claude-plugin/plugin.json` at repo root. Required `name: "lazyspec"`; also set `description`, `version`, `author`. Manifest at root so the loader auto-discovers components relative to root: existing `skills/` and (later) `hooks/hooks.json`. No `commands/` or `agents/` declared.
- `.claude-plugin/marketplace.json` at repo root. Marketplace named `lazyspec`. Required `name`, `owner` (`{ "name": "jkaloger" }`), `plugins` array. Single entry: `source: "."`, `strict: false` (required so the root `plugin.json` is honoured alongside the marketplace entry).
- Both files coexist under `.claude-plugin/`, each its own role.

## Out of scope

- `hooks/hooks.json` (S2).
- README docs (S3).
- Scratch-project install proof (S4).

## Acceptance criteria

- `/plugin marketplace add jkaloger/lazyspec` registers the marketplace.
- `/plugin install lazyspec@lazyspec` installs the plugin.
- After install, all ten skill directories under root `skills/` (`lazy`, `scaffold`, `co-write`, `generate`, `advance`, `execute`, `review`, `systematic-debugging`, `configure-type`, `create-audit`) are discovered; loose `skills/README.md` and `skills/MIGRATION-*.md` (no `SKILL.md`) are ignored.
- The root `plugin.json` is honoured (not shadowed by the marketplace entry) — confirms `strict: false` + `source: "."`.

## Notes

`source: "."` + `strict: false` is the less-trodden config. The spec-idiomatic alternative (plugin in a `plugins/` subdir with relative source) is rejected: a subdir plugin cannot reuse root `skills/` without a copy.

