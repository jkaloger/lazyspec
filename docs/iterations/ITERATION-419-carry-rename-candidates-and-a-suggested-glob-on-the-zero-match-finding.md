---
title: Carry rename candidates and a suggested glob on the zero-match finding
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-271
- blocks: ITERATION-420
---

## Objective

A `governs-no-match` finding on a document with `reviewed` names where the files went and what glob would catch them.

## Satisfies

STORY-271 AC1, AC2, AC3, AC4.

## Context

- Story + ACs: STORY-271
- Source of the pairs, the suggestion rule, and its accepted over-widening on a module split: RFC-068 §Design "Validation", §Decisions 5, §Risks "Suggested glob is a heuristic"
- The finding lands in ITERATION-413 with these fields empty; `reviewed` is stamped by ITERATION-418. Both are prerequisites.
- Touch:
  - `src/engine/git_ref.rs:11` -- add `renames(root, from, to)` over `git diff -M --name-status <from>..<to>`. Same seam, same fake pattern as `head`.
  - `src/engine/validation.rs` -- the `GovernsNoMatch` rule populates `renamed` and `suggested_glob` instead of leaving them empty.
- Filter matters: only pairs whose `from` the stale glob matched. A repo-wide rename list is not the answer (AC1).
- Suggestion is the longest common *directory* prefix of the `to` paths plus `/**`. Two directories collapse to their common ancestor, and every pair is still reported so the author can narrow by hand (AC4).
- No `reviewed` means no git call at all: `renamed` empty, `suggested_glob` null, and not an error (AC3).

## Tasks

1. Add `renames` to `GitRefOps` with a real implementation and a fake returning a fixed pair list.
2. Test-first: a stale glob plus `reviewed` plus a rename under that glob gives `renamed` holding that pair and no others (AC1).
3. Test-first: `suggested_glob` is the `to` directory plus `/**` for one directory, and the common ancestor plus `/**` for two, with both pairs still present (AC2, AC4).
4. Test-first: no `reviewed` gives empty and null, and `renames` is never called (AC3).
5. Implement.

## Out of scope

- `fix --governs` -- ITERATION-420.
- Touching `reviewed`. It stays put so RFC-069 still sees the drift.
- Narrowing the suggestion past common-prefix. The heuristic is the accepted design, not a defect.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-004: no real git in tests; the fake supplies the pairs.

## Verification

On a scratch branch: pin a document that governs a directory, rename that directory, commit, then `cargo run --quiet -- validate --json | jq '.warnings[] | select(.rule=="governs-no-match")'` shows the pairs and the suggested glob.
