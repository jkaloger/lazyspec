---
title: Non-blocking store locks on the TUI thread and editor arg parsing
type: iteration
status: complete
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-220
---

## Objective

UI thread never blocks on store mutex held across network sync; editor opens instantly; $EDITOR args work.

## Satisfies

STORY-220 AC1–AC4. (fixes BUG-001)

## Context

- Lock across whole sync: `poll_sync` (`src/tui/infra/event_loop.rs:304`, comment :301-303) incl `gh` fetches + `git fetch --prune` no timeout (`sync.rs:391` → `git_ref.rs:216-221`)
- UI-thread blocking locks: `gh_issue_map_stale` branch `event_loop.rs:751`, `try_push_gh_edit` `event_loop.rs:95`
- First poll immediate: `event_loop.rs:655-656`; every `cache_ttl` (60s default)
- Editor spawn clean: `run_editor` `event_loop.rs:49-65`; but `Command::new(&editor)` breaks `EDITOR="code --wait"`

## Tasks

1. Restructure poll_sync: sync into snapshot outside lock, swap under short lock — OR — UI-thread paths use `try_lock`, retry next tick. Pick per code shape; UI thread must never wait on network.
2. Timeouts on `git fetch`/`gh` subprocess calls in sync path.
3. `resolve_editor`/`run_editor`: whitespace-split $EDITOR into program + args.
4. Regression test: editor request proceeds while fake store holds lock. Editor-args unit test. `cargo test`.

## Out of scope

Poll scheduler rework. Async runtime.

## Verification

`cargo test`. Manual (Linux if avail): slow remote + `e` → editor instant.

