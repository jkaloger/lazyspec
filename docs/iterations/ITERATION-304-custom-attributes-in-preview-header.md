---
title: Custom attributes in preview header
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-217
---

## Objective

Preview/detail header lists doc's custom attributes.

## Satisfies

STORY-217 AC1–AC4.

## Context

- Touch: `build_preview_header_lines` (`src/tui/views/panels.rs:932-996` — shows title/type/status/author/date/tags/provenance, skips attributes)
- Data: `DocMeta.attributes` (`src/engine/document.rs:327`), format via `attr_value_display` (`panels.rs:1724-1734`)

## Tasks

1. Header lines: one `name: value` per attribute, `attr_value_display` formatting, after tags/provenance.
2. Empty map → no rows. `AttrValue::Raw` displays too.
3. Test: header lines include attrs; empty attrs unchanged. `cargo test`.

## Out of scope

Editing attrs in TUI. Table columns (ITERATION under STORY-216).

## Verification

`cargo test`. Manual: doc w/ attrs → visible in preview pane, all stores.

