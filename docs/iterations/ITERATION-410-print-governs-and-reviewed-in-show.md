---
title: Print governs and reviewed in show
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-265
---

## Objective

`show <id>` and `show <id> --json` print a document's pins.

## Satisfies

STORY-265 AC5. Closes the story.

## Context

- Story + ACs: STORY-265
- RFC-068 §Design "Lookup" last line is the whole requirement.
- Fields land in ITERATION-407.
- Touch: `src/cli/show.rs:93` `run`, `:187` `run_json`.
- Human output follows how `tags` and `provenance` render -- the row is absent when the field is empty, not an empty row.
- JSON keeps the shape agents already get from `provenance` (always present, empty list when unset) and `assignee` (`null` when unset).

## Tasks

1. Test-first: `show --json` on a pinned document carries `governs` and `reviewed`; on an unpinned one, `[]` and `null`.
2. Test-first: human `show` prints both when set and neither row when unset.
3. Implement.
4. README: the `show` output description, if it enumerates fields.

## Out of scope

- TUI and web detail rendering -- STORY-269 AC1, AC2.
- `why`, validation.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-006 on consistent JSON shapes.

## Verification

`cargo run --quiet -- show RFC-068 --json | jq '.governs, .reviewed'` gives `[]` and `null`; the same after pinning gives the authored values.
