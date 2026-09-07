---
title: Serialise validation findings as objects with a rule slug
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-266
- blocks: ITERATION-412
- blocks: ITERATION-413
- blocks: ITERATION-414
---

## Objective

`validate --json` emits `warnings` and `errors` as objects carrying `rule`, `message` and the variant's own fields. Message text and human output are byte-identical to today.

## Satisfies

STORY-266 AC1, AC2, AC3, AC4.

## Context

- Story + ACs: STORY-266
- Object shape, one slug per variant, why a string cannot carry repair data: RFC-068 §Design "Finding shape", §Interfaces `ValidationIssue::rule`, §Decisions 7, §Risks "`validate --json` breaks"
- Touch:
  - `src/engine/validation.rs:10` `ValidationIssue` -- `rule()` plus a serialisation carrying `rule`, `message` and the variant fields. Exhaustive match, no wildcard (DICTUM-001), so a later variant cannot ship without a slug.
  - `src/cli/validate.rs:68` -- `warnings` and `errors` are `format!("{}", w)` today. That string *is* `message`; AC2 is a byte-for-byte guarantee against it.
  - `src/cli/validate.rs:36` `gh_auth_warnings` produces plain strings with no variant behind them. Decide their slug and keep them in the array -- they are not `ValidationIssue`s and must not be dropped.
  - `src/cli/validate.rs:86` `run_human` -- must not change (AC3).
  - `src/tui/state/app.rs:923` builds `validation_warnings: Vec<String>` from `e.to_string()`. That is `message`, so the TUI half of AC4 is "still compiles, still renders", not a rewrite. Panel: `src/tui/views/overlays.rs:879`.
- The web has no validation view -- `src/web/server.rs:77` routes list, fragment, search, graph, doc, static only. The web half of AC4 is vacuous. Confirm and say so in the report; do not build one.
- `UnsatisfiedEdge` (`validation.rs:15`) is the variant STORY-264 wants field-serialised. Serialising its fields here is what closes that overlap.

## Tasks

1. Test-first in `validation.rs`: every variant has a distinct, non-empty `rule()`.
2. Add `rule()` over an exhaustive match.
3. Test-first in `validate.rs`: for a fixture with one warning and one error, each JSON object's `message` equals the `Display` string and `rule` is the variant's slug.
4. Implement the object serialisation, variant fields included.
5. Pin `run_human` output for the same fixture against a golden string (AC3).
6. Update every consumer that string-matches a finding: `tests/integration/`, the TUI panel path.

## Out of scope

- Skill and README prose -- ITERATION-412.
- Any `governs-*` rule -- STORY-267, STORY-268.
- Building a web validation view. RFC-068 §Non-goals has no new surfaces.
- Rewording any human message.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-001 on exhaustive matching; DICTUM-006 on consistent JSON. Engine change: check TUI and web for consumers before and after.

## Verification

Capture `cargo run --quiet -- validate --json | jq -c '.warnings, .errors'` on this repo before the change. After, `jq -c '[.warnings[].message], [.errors[].message]'` is identical to it.
