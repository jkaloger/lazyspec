---
title: Task status colours derived from ClickUp
type: story
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: RFC-056
---## Context

ClickUp store (RFC-056) derives a type's lifecycle status *names* from the bound List at sync (ITERATION-277) but drops the per-status `color` hex the ClickUp API returns — `ClickupStatus` (`src/engine/clickup.rs:157`) deserializes only `status`/`orderindex`/`type`, and `derive_lifecycle` (`src/engine/clickup_cache.rs:126`) keeps only names.

Every renderer hardcodes status colour by matching the *default-lifecycle* names: TUI `status_color()` (`src/tui/views/colors.rs:5`), CLI `status_style()` (`src/cli/style.rs:7`, plus `src/cli/context.rs`), web `[data-status="draft"]` selectors (`static/lazyspec.css`). ClickUp statuses (`pending`, `ready to start`, `ready to release`) match none → `Color::Reset` / no swatch. Users lose the status colour coding they already have in ClickUp.

This slice captures the colour ClickUp already sends and renders it, **automatically at sync** — same posture as the derived status names, no hand-authoring. Config-authored per-status colours for generic/filesystem docs is a separate, deferred feature and out of scope here.

Derived colour is presentation metadata, not structure: it lives in the gitignored cache (sibling to `.lazyspec/task-map.json`), never in committed `.lazyspec.toml`.

## Acceptance Criteria

- **Given** a clickup-tasks type bound to a List whose statuses carry `color`
  **When** sync runs
  **Then** each status's hex is captured and persisted to a gitignored cache artifact, not to `.lazyspec.toml`.

- **Given** a synced ClickUp doc
  **When** shown in the TUI
  **Then** its status renders in the ClickUp-derived hex colour.

- **Given** a synced ClickUp doc
  **When** shown via CLI (`show` / `context` / `status`)
  **Then** its status renders in the ClickUp-derived hex colour.

- **Given** a synced ClickUp doc
  **When** served in the web view
  **Then** its status swatch renders in the ClickUp-derived hex colour.

- **Given** a filesystem/GitHub doc with no derived colour
  **When** rendered on any surface
  **Then** the current hardcoded name→colour behaviour is unchanged.

- **Given** a ClickUp status with no colour or one not in the cache
  **When** rendered
  **Then** it falls back to the existing default behaviour — no crash, no regression.

- **Given** the derived-colour cache
  **When** inspected
  **Then** it is an on-disk JSON artifact (like `task-map.json`), readable programmatically.

## Scope

### In Scope

- Capture `color` on `ClickupStatus` (deserialize the field the API already sends).
- New gitignored cache artifact, sibling to `.lazyspec/task-map.json`, keyed `type → {status → hex}`, written at sync beside the lifecycle derivation in `fetch.rs`. Load/save mirrors `TaskMap`.
- Engine resolver: `(type, status) → Option<hex>`.
- Render in all three surfaces — TUI (`ratatui::Color`), CLI (`console::Style`), web (status swatch) — each falling back to today's name→colour map when no derived colour exists.
- hex→terminal-colour handling for TUI/CLI (truecolor with nearest-256 fallback); hex used directly for web.

### Out of Scope

- Config-authored per-status colours for generic/filesystem/GitHub docs — separate future feature.
- Any change to lifecycle gating, edges, or the derived-status-*names* path.
- Colour authoring commands/UI.
- Colour-downgrade fidelity beyond a reasonable truecolor + nearest-256 fallback.