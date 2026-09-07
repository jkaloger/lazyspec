---
title: Seed this repo pins and turn unowned on
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-268
---

## Objective

This repo's `.lazyspec.toml` carries a `[governs]` scope and severity it passes clean, with the pins authored to make that true.

## Satisfies

STORY-268 AC6. Closes the story.

## Context

- Story + ACs: STORY-268
- The rule lands in ITERATION-414.
- Pin at module-directory depth, never per file: RFC-068 §Design "Frontmatter". Adoption order -- seed pins, narrow `scope`, then turn `unowned` on: §Risks "Day-one flood". Why authoring pressure is the point: §Motivation 4.
- Touch: `.lazyspec.toml` `[governs]`; `governs` frontmatter on documents under `docs/convention/`, `docs/rfcs/`, `docs/specs/`.
- The dictums are the obvious first pins -- DICTUM-006 over `src/cli/**`, DICTUM-007 over `src/tui/**`, DICTUM-003 over module layout. Read what each one actually governs before pinning it; a `cli` tag is a hint, not the mapping.
- `scope` is the knob that keeps this honest. A narrow scope over genuinely pinned modules beats `src/**` propped up by invented pins.
- Authoring, not code. No Rust changes in this slice.

## Tasks

1. On a scratch copy, set `scope = ["src/**"]` and `unowned = "warning"` and run `validate --json`. That file list is the inventory. Do not commit that config.
2. Pin the modules that already have a document genuinely governing them.
3. Set `scope` to those modules and `unowned` to a severity this repo passes clean.
4. `validate` green.

## Out of scope

- Pinning every module, or writing new documents to cover unowned code. That is an ongoing authoring push, not a slice.
- Widening `scope` to all of `src/**`.
- Any change to the rule.

## Principles/conventions

`cargo run --quiet -- convention`. Convention principle 1: a pin exists because a document really governs that code, not to clear a finding.

## Verification

`cargo run --quiet -- validate --json | jq '[.warnings[], .errors[]] | map(select(.rule | startswith("governs-")))'` is `[]`. `cargo run --quiet -- why src/cli/show.rs --json` names at least one document.
