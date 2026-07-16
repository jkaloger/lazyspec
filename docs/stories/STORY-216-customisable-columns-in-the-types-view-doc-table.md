---
title: "Customisable columns in the types view doc table"
type: story
status: accepted
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Value

As a lazyspec user, the doc-list table in the types view shows the columns I choose — including custom attributes — instead of the hardcoded seven.

## Context

- Types-view doc table is hardcoded: gutter, tree, ID, DOC, STATUS, TAGS, PROVENANCE (`src/tui/views/panels.rs` `doc_table_widths`, header ~860).
- Graph view already has the pattern to copy: `[tui.graph] columns` (`GraphConfig`, `src/engine/config.rs`), where unknown column ids resolve as custom attribute names (`graph_column_cell`).

## Acceptance Criteria

- AC1: `.lazyspec.toml` supports column config for the types-view doc table (e.g. `[tui.table] columns = [\"status\", \"tags\", \"provenance\"]`), same id semantics as graph columns: built-ins plus any custom attribute name.
- AC2: default column set matches today's table — no visual change without config.
- AC3: gutter, tree, ID, DOC remain fixed leading columns; configured columns follow.
- AC4: `lazyspec config --json` exposes the column config; config write round-trips it.
- AC5: README documents the config block.

## Out of scope

Interactive column toggling from inside the TUI; per-type column sets.
