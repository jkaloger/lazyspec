---
title: TUI stays responsive during background sync
type: story
status: in-progress
author: agent
date: 2026-07-17
tags: []
related:
- related-to: BUG-001
---

## Value

As a TUI user, background sync never blocks the UI thread — opening $EDITOR or navigating is instant even while a slow remote syncs (BUG-001).

## Acceptance Criteria

- AC1: no code path holds the shared store mutex across network I/O that the UI thread also locks; sync builds a snapshot and swaps under a short lock, or UI-thread lock attempts are non-blocking (`try_lock` + retry next tick).
- AC2: `git fetch`/`gh` subprocess calls in the sync path have timeouts.
- AC3: $EDITOR values with arguments (`code --wait`) spawn correctly (whitespace split).
- AC4: regression test: editor request proceeds while a fake store holds the lock.

## Out of scope

Reworking the poll scheduler; async runtime adoption.
