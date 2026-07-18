---
title: GitHub-backed docs inherit remote status
type: story
status: in-progress
author: unknown
date: 2026-07-18
tags: []
related:
- related-to: BUG-008
---

## Value

As a lazyspec user with github-issues or milestone-backed types, doc status reflects the remote counterpart's state — closed issue/milestone shows closed-equivalent lifecycle state after sync — so lazyspec never disagrees with GitHub about where work stands.

## Acceptance Criteria

- AC1: sync maps remote state → lifecycle state: issue/milestone `open` → type's first active state, `closed` → type's terminal state (default mapping; custom lifecycles use their own first/terminal states).
- AC2: lazyspec-initiated transitions into a terminal state close the remote issue/milestone (write direction, same write-through model as body).
- AC3: transition initiated on the remote (issue closed on GitHub) surfaces after `sync`/TUI poll without local edits.
- AC4: filesystem/git-ref types unaffected.

## Out of scope

- Per-state custom mapping config (open/closed binary only, this slice).
- ClickUp (status colours already derived; full lifecycle inheritance later).
