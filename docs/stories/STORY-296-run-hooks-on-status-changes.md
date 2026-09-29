---
title: Run hooks on status changes
type: story
status: complete
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-075
reviewed: 85e41fa2ed48129833247457cbdc2a43f23c3e50
---

## Value

As a project maintainer, I add `pre-transition` hooks that can block a status change or update other documents as part of it, so a document can't reach a status until it meets my rules, and moving it can change related documents all at once, in any store.

## Acceptance Criteria

- AC1: `event = "pre-transition"` with optional `types`, `from` and `to` runs on a matching status change from `update --status` or the TUI. It receives the document being moved, `transition { from, to }`, and the documents of any `context_types`. Hooks run in the order they are declared.
- AC2: Any error finding blocks the move and stops the remaining hooks. The CLI exits non-zero with the findings (also in `--json`). The TUI shows the findings and leaves the status as it was. Warning findings are reported and the move still happens.
- AC3: `updates` entries (`id`, optional `part`, `hash`, `body`) are all checked (the document exists, its hash matches, the update targets body only) before any is saved, then saved together with the status through the `update --body` / `--part` engine path. Any failure saves nothing.
- AC4: `hook run <event> <id> [--dry-run] [--json]` is a CLI command that fires either event without changing status (for `pre-transition`, `from` and `to` are both the current status). `--dry-run` saves nothing.

## Out of scope

Creating documents from hooks. `post-*` events.
