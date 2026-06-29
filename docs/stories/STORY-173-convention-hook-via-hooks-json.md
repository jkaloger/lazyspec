---
title: Convention hook via hooks.json
type: story
status: complete
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: RFC-051
- blocks: STORY-174
- blocks: STORY-175
---

## Story

As a lazyspec user, I want the project convention injected into my agent context automatically on every prompt, so that I no longer hand-edit `.claude/settings.json` to wire a `UserPromptSubmit` hook.

This story adds the convention hook to the plugin. Depends on S1 (the manifest that makes `hooks/hooks.json` auto-discoverable).

## Scope

- `hooks/hooks.json` at repo root. Top-level shape: `{ "hooks": { "UserPromptSubmit": [ { "hooks": [ { "type": "command", "command": "..." } ] } ] } }`. Matcher omitted (matches all).
- Command: `lazyspec convention --preamble 2>/dev/null || true`. On success, preamble flows to stdout and is injected into model context. With no `.lazyspec.toml`, `convention --preamble` exits 1 with a "no config" message; `2>/dev/null` suppresses stderr, `|| true` forces exit 0, stdout empty — a clean noop.
- Command runs under `/bin/sh -c` (Claude Code shell-form hooks), where `|| true` is portable. No `${CLAUDE_PLUGIN_ROOT}` — invokes the on-PATH binary, not a bundled script.

## Out of scope

- Hardening the CLI to exit 0 + empty output when no config (deferred alternative to the shell guard).
- Lease-coordination hooks in `hooks/claude-code-settings.json` (different filename, not auto-loaded, coexists).

## Acceptance criteria

- In a project WITH `.lazyspec.toml`: submitting a prompt injects the `convention --preamble` output into context.
- In a project WITHOUT `.lazyspec.toml`: prompt submit produces no error, no blocked prompt, no injected text — silent noop.
- `hooks/claude-code-settings.json` is unaffected; both files coexist.

## Notes

Per-prompt token cost in any `.lazyspec.toml` project is accepted — that is the point of the hook (current repo behavior).

