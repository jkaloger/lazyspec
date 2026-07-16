---
title: Config-driven status colours with stable unknown-status fallback
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-215
---

## Objective

Per-status colour config in `.lazyspec.toml`; unknown statuses get stable colour, never `Color::Reset`.

## Satisfies

STORY-215 AC1–AC5.

## Context

- Touch: `src/tui/views/colors.rs:17` (`status_color` — hardcoded match, `_ => Color::Reset` at :32), `src/engine/config.rs:465-475` (`UiConfig` — add colour map), `src/engine/config_write.rs` (round-trip), `src/engine/status_colors.rs` (ClickUp cache — keep, lower precedence than config)
- Fallback pattern: `tag_color` hash palette (`colors.rs:36-53`)

## Tasks

1. `UiConfig`: `status_colors: BTreeMap<String, String>` (`[tui.status_colors]`, status → named ANSI or `#hex`). Parse + default empty.
2. Colour parse fn: named ANSI + hex (reuse `hex_to_color`). Invalid value → warning, skip.
3. `status_color` resolution: config → ClickUp cache → built-in match → hash palette (like `tag_color`), never `Color::Reset`.
4. `config --json` exposes map; `config_write` round-trips it. Round-trip test.
5. README: `[tui.status_colors]` block.
6. Tests: config colour wins, unknown status stable colour, invalid hex skipped. `cargo test`.

## Out of scope

Tag/panel theming. Per-type status colours.

## Verification

`cargo test`. Manual: custom lifecycle type, custom status → coloured in doc table + graph.

