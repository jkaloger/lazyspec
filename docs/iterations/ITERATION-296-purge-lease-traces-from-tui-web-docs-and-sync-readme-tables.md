---
title: Purge lease traces from TUI-web-docs and sync README tables
type: iteration
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-208
---

## Objective

Purge lease traces from TUI/web/docs; sync README command + keybind tables to shipped surface.

## Satisfies

STORY-208 AC5, AC6.

## Context

- Blocked by: lease engine/CLI removal iteration (must land first).
- TUI: `src/tui/state/app.rs` (`FieldPath::CoordinationLeaseDuration`), `src/tui/views/panels.rs` (config-panel row + edit path)
- Web: `src/web/server.rs` (any lease/coordination surfacing)
- Docs: `README.md` (lease/coordination refs incl. hook snippets :372-392; command table :262-289; keybind table :156-171), `CHANGELOG.md`
- Drift detail: AUDIT-018 F8 (missing commands: fetch, config, convention, skills, completions, `fix --renumber/--type`; missing keys: s p a x g G Space Tab per `src/tui/views/keybinds.rs:350-410`)

## Tasks

1. Remove TUI coordination field + panel row + edit path.
2. Remove web lease surfacing (if any remains post engine removal).
3. README: strip lease/claim/heartbeat/coordination; add missing command rows; sync keybind table from keybinds.rs registry.
4. CHANGELOG: breaking-change note + orphaned-refs prune one-liner (RFC-061 Migration).
5. `cargo build && cargo test`.

## Out of scope

Any engine/CLI code (prior iteration). New README sections beyond table sync.

## Verification

`grep -i "lease\|coordination" README.md src/tui src/web` → zero hits. Keybind table rows == `bind!` entries in keybinds.rs.

