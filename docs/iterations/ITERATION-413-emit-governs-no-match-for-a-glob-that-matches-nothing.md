---
title: Emit governs-no-match for a glob that matches nothing
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-267
- blocks: ITERATION-419
---

## Objective

`validate` warns once per `governs` glob that matches zero files under root.

## Satisfies

STORY-267 AC1, AC2, AC3, AC4, AC5. Closes the story.

## Context

- Story + ACs: STORY-267
- Variant fields and fixed warning severity: RFC-068 §Design "Validation", `GovernsNoMatch`
- Compiled globs land in ITERATION-408; object findings in ITERATION-411. Both are prerequisites.
- Touch: `src/engine/validation.rs` -- new variant, `Display` arm, `rule()` arm (`governs-no-match`), and a rule struct registered where the others are. ITERATION-406 removed `StatusConsistencyRule` from that registry; read what it looks like now rather than the pre-removal shape.
- `renamed` and `suggested_glob` are on the variant from day one but stay empty and `None` here. STORY-271 fills them. Do not stub a git call.
- Per glob, not per document: three globs with one miss is one finding (AC4).
- Matching a glob against the filesystem needs a walk under `governs.root`. Reuse the walker the store already loads with; do not add one.

## Tasks

1. Test-first: one glob matching zero files gives one finding naming the document path and the glob (AC1).
2. Test-first: a glob with at least one match gives none (AC3); three globs with one miss give exactly one (AC4).
3. Add the variant, its `Display` string and its `rule()` slug.
4. Implement the rule, register it.
5. Test-first in `cli/validate.rs`: the `--json` finding carries `rule`, `path`, `glob`, `message` (AC2).

## Out of scope

- Populating `renamed` / `suggested_glob`, and `fix --governs` -- STORY-271.
- `governs-unowned` -- STORY-268.
- A TUI or web change. The finding rides the existing warnings panel (`src/tui/views/overlays.rs:879`) with no code change, and the web has no validation view. Confirm AC5, do not build for it.

## Principles/conventions

`cargo run --quiet -- convention`. Follow the last validation rule added, per DICTUM-003 §Conventions.

## Verification

Pin a scratch document in this repo at `governs: ["src/nope/**"]`. `cargo run --quiet -- validate --json | jq '.warnings[] | select(.rule=="governs-no-match")'` names it with that glob. Remove the pin, the finding goes.
