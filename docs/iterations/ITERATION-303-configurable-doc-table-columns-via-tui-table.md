---
title: Configurable doc-table columns via tui.table
type: iteration
status: complete
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-216
---

## Objective

`[tui.table] columns` config drives types-view doc table; defaults match today.

## Satisfies

STORY-216 AC1–AC5.

## Context

- Touch: `src/tui/views/panels.rs:263` (`doc_table_widths` — fixed 7), header `panels.rs:860-868`, `DocCellWidths` `panels.rs:341-363`
- Copy pattern: `GraphConfig` (`src/engine/config.rs:489-495`), `graph_column_cell` (`panels.rs:1711-1721` — unknown id = custom attr name)

## Tasks

1. `TableConfig` in `UiConfig`: `columns: Vec<String>`, default `["status", "tags", "provenance"]` (today's set). `[tui.table]` TOML.
2. Doc table render: fixed gutter/tree/ID/DOC + configured columns. Column cell resolution shared w/ `graph_column_cell` (extract helper — two uses now).
3. Width calc: dynamic per configured column count.
4. `config --json` exposes; `config_write` round-trips. Round-trip test.
5. README: `[tui.table]` block.
6. Tests: default render unchanged, custom attr column shows value. `cargo test`.

## Out of scope

Interactive column toggle. Per-type columns. Graph view (already configurable).

## Verification

`cargo test`. Manual: no config → identical table; `columns = ["status", "priority"]` → attr column renders.

