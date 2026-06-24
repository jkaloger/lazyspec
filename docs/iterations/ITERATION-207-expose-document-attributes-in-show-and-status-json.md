---
title: Expose document attributes in show and status JSON
type: iteration
status: draft
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: STORY-152
---<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Changes

- Depends ITERATION-205 (`DocMeta.attributes`).
- `AttrValue`: derive `Serialize` -> JSON (Int->number, Date->`YYYY-MM-DD` string, Raw->passthrough).
- `src/cli/show.rs` (show `--json` builder): add `"attributes"` field from `DocMeta.attributes`. Empty map -> `{}`.
- `src/cli/status.rs` (status `--json`, per-doc entry): add `"attributes"` per document. Empty -> `{}`.
- README: note `attributes` field in show/status JSON schema.

## Test Plan

- AC1: `show <id> --json` w/ attrs -> `.attributes` typed (int number, enum string, date string). cli integration.
- AC2: `status --json` -> each doc entry has `.attributes`. cli integration.
- AC3: doc w/o attrs -> `.attributes == {}` present, stable shape. cli integration.

## Notes

- Principle 2: agents read same attrs as TUI.
- Stable empty `{}` so consumers needn't null-check.
