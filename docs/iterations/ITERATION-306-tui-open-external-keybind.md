---
title: TUI open-external keybind
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-219
---

## Objective

TUI keybind opens selected doc externally (browser or viewer cmd).

## Satisfies

STORY-219 AC1, AC3 (TUI half), AC4, AC5.

## Context

- Depends: open-target resolution iteration (engine::ops `resolve_open_target`)
- Spawn pattern: `run_editor` (`src/tui/infra/event_loop.rs:49-65` — suspend/resume terminal)
- Keybinds: `src/tui/views/keys.rs` (doc-view keys ~533-537)

## Tasks

1. Keybind (`o`) on selected doc → `resolve_open_target`.
2. Url → detached browser spawn (no terminal suspend). File → viewer cmd w/ suspend/resume à la `run_editor`; viewer arg whitespace-split.
3. Neither resolves → status-line message, no panic.
4. README keybind table + help overlay.
5. Tests: key routes to request, target dispatch. `cargo test`.

## Out of scope

Embedded rendering. Web view (already links out).

## Verification

`cargo test`. Manual TUI: `o` on fs doc → glow; on github-issues doc → browser.

