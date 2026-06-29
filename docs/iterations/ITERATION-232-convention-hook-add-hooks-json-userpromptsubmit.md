---
title: 'Convention hook: add hooks.json UserPromptSubmit'
type: iteration
status: draft
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: STORY-173
---

## Goal

Convention hook in plugin. `UserPromptSubmit` injects `convention --preamble`. Silent noop outside lazyspec project.

## Tasks

- Add `hooks/hooks.json` at root. Shape: `{ "hooks": { "UserPromptSubmit": [ { "hooks": [ { "type": "command", "command": "lazyspec convention --preamble 2>/dev/null || true" } ] } ] } }`. Matcher omitted → matches all.
- Guard rationale: no config → `convention --preamble` exit 1 + stderr msg. `2>/dev/null` drop stderr. `|| true` → exit 0. stdout empty → nothing injected.
- Runs under `/bin/sh -c` (shell-form hook, no `args`). `|| true` portable there. No `${CLAUDE_PLUGIN_ROOT}` (on-PATH binary, not bundled script).
- Verify inject: project WITH `.lazyspec.toml` → preamble in context on prompt submit.
- Verify noop: project WITHOUT `.lazyspec.toml` → no error, no block, no text.
- Confirm `hooks/claude-code-settings.json` (lease coord) untouched, coexists (diff filename, not auto-loaded).

## Acceptance criteria

- WITH config → preamble injected on `UserPromptSubmit`.
- WITHOUT config → silent noop (exit 0, empty stdout).
- `hooks/claude-code-settings.json` unaffected.

## Out of scope

- CLI hardening (exit 0 + empty when no config). Shell guard chosen; CLI fallback deferred.
- Lease-coordination hooks bundling.

## Depends

STORY-172 (manifest makes `hooks/hooks.json` auto-discoverable).

