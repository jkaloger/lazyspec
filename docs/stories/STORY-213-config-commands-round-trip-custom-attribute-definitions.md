---
title: Config commands round-trip custom attribute definitions
type: story
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- related-to: AUDIT-018
---

## Value

As a user configuring a custom type with attributes, `lazyspec config` commands write valid TOML — no hand-editing, no repo-breaking configs (root cause of the 2026-07-16 outage, AUDIT-018 F1).

## Acceptance Criteria

- AC1: `update_type_table` (config_write) serializes `TypeDef.attributes` as `[[types.attributes]]` sub-tables (name, kind, required, values), and never emits a conflicting inline `attributes = []` alongside them.
- AC2: `config add-type` accepts attribute definitions (flag or stdin JSON — follow existing add-type input style).
- AC3: round-trip test: config with attribute-bearing type → `write_config_in_place` → reparse → identical `AttrDef`s.
- AC4: regression test: the duplicate-key shape from commit c83bb99 (inline `attributes = []` + `[[types.attributes]]`) is covered — `fix --config` or load gives an actionable error naming the type.
- AC5: the working-tree `.lazyspec.toml` fix (inline `attributes = []` removed from `bug` type) is committed.

