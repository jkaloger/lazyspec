---
title: Rewrite stale globs with fix --governs
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-271
---

## Objective

`fix --governs` rewrites every stale glob to its suggestion, in place, leaving `reviewed` alone.

## Satisfies

STORY-271 AC5, AC6, AC7. Closes the story.

## Context

- Story + ACs: STORY-271
- Why `fix` applies rather than only suggesting, and why `reviewed` deliberately stays put: RFC-068 §Design "Finding shape" final paragraph, §Decisions 5
- The suggestion lands in ITERATION-419.
- Touch: `src/cli/fix.rs:45` `run`, `:169` `run_json`, `:195` `run_human`; `src/cli/fix/output.rs` for reporting; `src/engine/ops/fix/` for the rewrite. `fix --config` (`src/cli/fix.rs:81`, `:102`, `:107`) is the precedent for a `fix` sub-mode with dry-run and JSON -- follow its shape rather than inventing a second one.
- Findings with no `suggested_glob` are skipped, not errors.
- The rewrite replaces one entry in the `governs` list and leaves the rest untouched, through the same frontmatter writer `pin` uses (ITERATION-418).

## Tasks

1. Test-first: a document with a stale glob and a suggestion has that entry rewritten, with its other `governs` entries and its `reviewed` unchanged (AC5).
2. Test-first: `--json` lists document, old glob and new glob per rewrite (AC6).
3. Test-first: `validate` after the fix has no `governs-no-match` for that glob (AC7).
4. Implement, matching the `--config` sub-mode shape including dry-run if `fix` has one.
5. README: `fix --governs`.

## Out of scope

- Re-stamping `reviewed`. Deliberate -- RFC-069 must still see the drift after the repair.
- Interactive selection of which globs to rewrite.
- Findings with no suggestion, and `governs-unowned`.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-006 on `--json` and command module shape.

## Verification

The scratch rename from ITERATION-419: run `fix --governs`, then `validate` is clean of `governs-no-match`, and `git diff` shows one changed glob line and no change to `reviewed`.
