---
title: GitHub-native blocks read-back on fetch
type: iteration
status: in-progress
author: jkaloger
date: 2026-07-22
tags: []
related:
- implements: STORY-244
---

## Objective

`fetch` reads GitHub-native issue dependencies back into the graph as `blocks`/`blocked-by`.

## Satisfies

STORY-244 AC3, AC6 (read side).

## Context

- Story + ACs + design: [[STORY-244]] §Design (read path)
- Touch:
  - `src/engine/gh.rs` — dependency-list read op on the REST seam (extends what the write iteration landed)
  - `src/engine/sync.rs` — on issue fetch, read native deps and inject relations, mirroring milestone read-back (sync.rs:558) and membership read-back (sync.rs:737)

## Tasks

1. `gh.rs`: dependency-list read op (an issue's native `blocked_by` set) if not already added by the write iteration.
2. `sync.rs`: on issue fetch, read native deps; inject `A blocked-by B` / `B blocks A` per the relation's declared inverse. Mirror milestone injection resolution/ordering.
3. Test-first: fake returns a native dependency; assert `show --json` / `status --json` surface `blocks`/`blocked-by` with the correct inverse direction (no output-shape change).

## Out of scope

- Write path (link/unlink native edge) — prior iteration.
- Cross-repo native deps.

## Principles/conventions

- Same as the write iteration: engine owns read-back; principle 4 fake at trait seam; `type-driven-design`, `testing`.

## Verification

- Native dependency set out-of-band on GitHub → `fetch` → `show --json` shows `blocks`/`blocked-by` in correct direction (AC3), exercised via the fake (AC6).

