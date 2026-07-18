---
title: GitHub-backed docs seed first lifecycle state at birth
type: iteration
status: complete
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-221
- related-to: BUG-007
- blocks: ITERATION-318
---

## Objective

GitHub-backed status derivation seeds type first lifecycle state (`lifecycle.states[0]`), same as filesystem/git-ref after ITERATION-312 — not hardcoded `draft`. Fixes BUG-007.

## Satisfies

BUG-007 (defect under STORY-221 AC1/AC4 — create seeds valid lifecycle state across stores).

## Context

- Layer: engine only (src/engine).
- Bug: github-backed status derivation hardcodes `Status::new("draft")` (src/engine/github_url.rs:183), predates per-type lifecycles → github doc born outside its lifecycle (e.g. type starting `reported` surfaces `draft`).
- Prior art to mirror: ITERATION-312 (STORY-221 AC1) seeded `TypeDef.lifecycle.states[0]` for fs/git-ref create. Route github path through same seeding. Default lifecycle still `draft` (states[0]) → no regression there.
- Overlaps BUG-008 (remote-state inheritance) — this iteration only fixes birth-state seeding; BUG-008 read/write-through builds on top (blocks it).

## Tasks

1. Test-first: engine test — github-backed type w/ lifecycle starting `reported` → derived status == `reported`, NOT `draft`. Default-lifecycle github type → still `draft` (states[0]).
2. Replace hardcoded `Status::new("draft")` at github_url.rs:183 with `TypeDef.lifecycle.states[0]` seeding (reuse ITERATION-312 helper/path; do not duplicate lifecycle lookup).
3. Confirm no other github-backed `draft` literal remains in status derivation.

## Out of scope

- Remote open/closed → lifecycle mapping and write-through close (BUG-008 / STORY-223 — separate iterations, blocked by this).
- fs/git-ref create (already done, ITERATION-312).

## Principles/conventions

- CLAUDE.md: engine, no I/O assumptions. Single status-model source — reuse ITERATION-312 seeding, resolve once for both bugs.

## Verification

`create <github-type> x --json` → `"status": "<states[0]>"` (e.g. `reported`). Default-lifecycle type still `draft`. `cargo test`.
