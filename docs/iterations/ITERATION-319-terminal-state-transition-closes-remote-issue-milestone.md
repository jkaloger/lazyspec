---
title: Terminal-state transition closes remote issue/milestone
type: iteration
status: complete
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-223
- related-to: BUG-008
---

## Objective

lazyspec-initiated transition into terminal lifecycle state closes the remote github issue/milestone (write direction, write-through model). Fixes BUG-008 write half.

## Satisfies

STORY-223 AC2. (Read-direction AC1/AC3/AC4 in sibling iteration.)

## Context

- Layer: engine only (src/engine) — github-backed store write path.
- Bug: lazyspec tracks status independently of remote; transitioning a github doc to terminal state does NOT close the remote issue/milestone (BUG-008).
- Write-through model: same as body write-through already used for github-backed docs — transition into terminal state → close remote counterpart via GitHub API.
- Terminal state = last/terminal of `TypeDef.lifecycle.states` (mapping defined in read-direction iteration ITERATION-318) → reuse that mapping, blocked-by it.

## Tasks

1. Test-first: engine test — transition github-issue-backed doc into terminal state → remote close invoked (fake/mock GitHub client asserts close call); milestone likewise. Transition into non-terminal state → no close. fs/git-ref → no remote call (AC4 boundary).
2. Hook advance/update terminal-state transition to remote-close through existing github write-through path.
3. Reuse first-active/terminal mapping from ITERATION-318 (no duplicate mapping logic).

## Out of scope

- Reopen-on-leave-terminal (STORY-223 covers close direction only).
- Read-direction inherit (ITERATION-318).
- Per-state custom mapping config; ClickUp.

## Principles/conventions

- CLAUDE.md: engine no I/O beyond store abstraction; mirror existing body write-through. Single mapping source shared w/ ITERATION-318.

## Verification

Advance github-issue doc to terminal state → issue closed on GitHub. Non-terminal transition → no close. `cargo test`.
