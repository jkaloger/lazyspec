---
title: Mark drifted records on why
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-272
---

## Objective

`why <path> --json` marks each governing record `drifted`, and no command outside `show` and `why` pays for a band.

## Satisfies

STORY-272 AC7, AC8. Closes the story.

## Context

- Story + ACs: STORY-272
- `drifted` on the `why` record: RFC-069 §Design "Outputs", last paragraph. On-demand-only: §Design "Computation", §Decisions 2.
- Touch:
  - `src/cli/why.rs:12` `entry` gains `drifted`; `run_json` (`:23`) and `run` (`:57`) need `config`, `root` and a `&dyn GitRefOps`, threaded from `src/main.rs:515`.
  - `src/cli/json.rs:35` `doc_to_json` -- the shape `list`, `context`, `status` and `search` share. It gains nothing.
- `drifted` is `compute(...).drift.files > 0`. A record with no `reviewed` has zero drift by the previous slice's fallback, which is AC7's second half -- no second rule for it here.
- One `compute` per record, not one per store. `governing` (`src/engine/store.rs:215`) already returns only the matching documents.
- AC8 is mostly held by construction: `Store::load` takes no git ops, so store load cannot issue a subprocess. What is worth a test is that `doc_to_json` carries no `staleness` key, so no command reading it acquired one.

## Tasks

1. Thread `config`, `root` and a `&dyn GitRefOps` into `why::run` and `run_json`.
2. Test-first: a record whose governed files moved since `reviewed` is `drifted: true`; unchanged is `false`; no `reviewed` is `false` and calls no git (AC7).
3. Test-first: `doc_to_json` has no `staleness` key (AC8).
4. Implement.
5. README §`why`: the `drifted` field.

## Out of scope

- Human `why` output. AC7 names `--json`; the card list stays as it is.
- The `stale` validation finding -- STORY-273. `validate` is exempt from AC8 for exactly that reason.
- TUI and web -- STORY-275. Neither reads `why`.

## Principles/conventions

`cargo run --quiet -- convention`. Principle 6: `drifted` is a field on the record that already exists, not a second staleness surface.

## Verification

`cargo run --quiet -- why src/engine/store.rs --json | jq '.[] | {id, drifted}'`, then `cargo run --quiet -- list --json | jq -e 'map(has("staleness")) | any | not'`.
