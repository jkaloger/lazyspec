---
title: Warn when a bundle is missing a declared part
type: story
status: accepted
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-074
reviewed: a6983d9cb6cbf9c5c2a647cfca0703ff0b38da8b
---

## Value

As a lazyspec user, I see when a document's folder lacks a part its template declares, so an incomplete change (no `design.md` yet) surfaces in `validate` and the TUI before review, not when someone opens it.

## Acceptance Criteria

- AC1: New validate rule `missing-part`. For each document whose type template is a directory, every file the directory declares other than `index.md` (parts and sidecars) that is absent from the document folder yields a warning `{path, part}`.
- AC2: `validate --id <id>` scopes the rule to that document. `validate --json` and `status --json` carry the finding with the same shape.
- AC3: A type with a flat-file template never produces `missing-part`. Extra parts never produce a finding. Severity is always warning; no per-part config.
- AC4: Older documents of a type whose template later became a directory get the warning. No migration, no ignore.
- AC5: TUI warnings panel lists `missing-part` findings alongside existing warnings.

## Out of scope

Per-part severity (wrap `validate --json` in CI for a hard error). Flagging a part whose `# Title` matches no template part (deferred in RFC-074).
