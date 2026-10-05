---
title: Show inverse relations on a document
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-042
---

## Value

As a reader of a document, I want `show` to list relations pointing at it, so I can see what folds into, supersedes or implements this document without searching from the other end.

## Acceptance Criteria

- AC1: `show <id>` text prints a section of inbound relations, each as the declared inverse keyword and the source (`implemented-by: STORY-297`). Symmetric relations print once.
- AC2: `show --json` adds an `inverse_related` array of `{type, target}`, always present, empty when none. `type` is the inverse keyword; a relation with no declared inverse uses the canonical name.
- AC3: The TUI Relations tab lists inbound relations under the same keywords.
- AC4: Inverse keywords come from `[[relationships]]` config, never hard-coded.

## Scope

### In Scope

- `Store::reverse_links_for` (`src/engine/store.rs:854`) as the data source; `Config::inverse_of` (`src/engine/config.rs:2437`) for the keyword.
- `doc_to_json` (`src/engine/doc_json.rs:35`) emits only the forward `related` today, so inbound edges are invisible to `show` and `show --json`.

### Out of Scope

- Web view: removed by ADR-037. AGENTS.md still mentions it.
- Editing inbound relations from `show`.
- Changing `context` traversal.

## Notes

The TUI Relations tab (`relation_sections`, `src/tui/state/app.rs:2905`) is built from `resolve_chain` plus declared `related`, so it shows inbound edges only where traversal follows them. Inbound edges otherwise appear only in the delete-confirm dialog (`referenced_by`, `src/tui/state/app.rs:3230`).
