---
title: Dry-run pre-transition hooks against a target status
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-075
---

## Value

As a pack author, I want `hook run pre-transition <id> --to <status>` to fire hooks as for a real transition, so I can dry-run a gate that only matches `from`/`to` edges.

## Acceptance Criteria

- AC1: `--to <status>` runs matching hooks with `from` = the document's current status and `to` = the given status. Hook payload `transition` carries both.
- AC2: `--to` naming a status with no lifecycle edge from the current status fails with the legal targets listed. No hook runs.
- AC3: Without `--to`, behaviour is unchanged: `from == to == current` (`run_hooks_by_hand`, `src/engine/ops/update.rs:~262`).
- AC4: The document's status is never changed, with or without `--dry-run`. `--json` echoes `from` and `to`.
- AC5: `--to` is rejected for `validate` events.

## Scope

### In Scope

- New `--to` on `hook run` (`src/cli/hook.rs:29`).
- Edge check against the type's `Lifecycle.edges` (`src/engine/config.rs:615`).

### Out of Scope

- Applying the transition. `update --status` stays the only mover.
- Skipping the edge check.
