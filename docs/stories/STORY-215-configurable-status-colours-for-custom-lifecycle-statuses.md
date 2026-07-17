---
title: "Configurable status colours for custom lifecycle statuses"
type: story
status: in-progress
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Value

As a lazyspec user with custom lifecycle statuses, every status renders with a colour in the TUI. Today unknown statuses fall through to `Color::Reset` (no colour) — only the seven built-in status names get colours, plus a ClickUp-synced hex cache.

## Context

- Resolution today: `src/tui/views/colors.rs` `status_color` — ClickUp sidecar cache (`.lazyspec/status-colors.json`) first, then hardcoded match on draft/review/accepted/in-progress/complete/rejected/superseded, else `Color::Reset`.
- No colour/theme surface in `.lazyspec.toml` — `[tui]` block has only ascii_diagrams/statusbar/multiline/graph.

## Acceptance Criteria

- AC1: `.lazyspec.toml` supports per-status colour config (e.g. `[tui.status_colors]` map of status name → colour: named ANSI or hex).
- AC2: resolution order: config colour → synced cache (ClickUp) → built-in defaults → deterministic fallback for unknown statuses (not `Color::Reset`).
- AC3: unknown custom statuses without config still get a stable, visible colour (e.g. hashed palette like tags).
- AC4: `lazyspec config --json` exposes the colour map; config write round-trips it.
- AC5: README documents the config block.

## Out of scope

Full TUI theming (borders, panels, tag palette).
