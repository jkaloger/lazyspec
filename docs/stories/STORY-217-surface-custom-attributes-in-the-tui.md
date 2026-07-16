---
title: "Surface custom attributes in the TUI"
type: story
status: accepted
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Value

As a lazyspec user with custom attribute definitions, I can see a document's attributes in the TUI. Today they render nowhere except graph-view columns when explicitly configured.

## Context

- Attributes parsed and stored on `DocMeta.attributes` (`src/engine/document.rs`), declared via `AttrDef` in config.
- Preview/detail header (`build_preview_header_lines`, `src/tui/views/panels.rs` ~932) shows title/type/status/author/date/tags/provenance but skips attributes.
- Graph view renders attributes only as opt-in columns (`graph_column_cell`).

## Acceptance Criteria

- AC1: preview/detail pane header lists a document's custom attributes (name: value) using `attr_value_display` formatting.
- AC2: attributes with no value on the doc are omitted (no empty rows).
- AC3: declared-but-Raw (undeclared) attribute values still display.
- AC4: works for every store backend (values come from DocMeta, store-agnostic).

## Out of scope

Editing attributes from the TUI; attribute columns in the doc table (covered by the columns story).
