---
title: Check document bodies with validate hooks
type: story
status: complete
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-075
reviewed: fe254c0889c93c166872a29e4081b64c557ad2f7
---

## Value

As a project maintainer, I add a `validate` hook to `.lazyspec.toml` and see its findings wherever lazyspec shows validation, so I can check my own rules for document bodies without changing lazyspec. Someone who clones the repo runs none of it until they trust it.

## Acceptance Criteria

- AC1: A `[[hooks]]` entry with `event = "validate"`, `run` (an argv array) and optional `types` runs once per validate, with every matching document as JSON on stdin (the `show --json` shape, including parts and a content hash).
- AC2: The hook's `findings` (`id`, `part`, `line`, `severity`, `message`) appear in `validate`, `status`, the TUI, in human and `--json` output, looking the same as built-in findings and naming the hook.
- AC3: A non-zero exit, invalid JSON, a timeout (30s default, `timeout` per hook) or any `updates` in the output becomes one error finding naming the hook, including its stderr.
- AC4: Until the hooks are trusted, every hook is skipped and one warning names `hook trust`. `hook trust` records a hash of the `[[hooks]]` table and of each in-repo `run` target, in user-local state outside the repo. Editing either one makes the hooks untrusted again.
- AC5: `hook list [--json]` shows each hook's name, event, scope and trust state. `--no-hooks` skips all hooks.
- AC6: The TUI's quick validation refresh shows the hook findings from the last full pass and spawns no hook processes. A full validate refreshes them.

## Out of scope

Transitions (STORY-296). Packs (STORY-297). Per-user trust scope.
