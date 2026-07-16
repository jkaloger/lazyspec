---
title: Config-write attribute serialization and add-type attribute support
type: iteration
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-213
---

## Objective

config-write serializes attribute defs; add-type accepts them; duplicate-key config shape caught; broken `.lazyspec.toml` fix committed.

## Satisfies

STORY-213 AC1–AC5.

## Context

- Finding: AUDIT-018 F1 (root cause + outage)
- Touch: `src/engine/config_write.rs:72` (`update_type_table` — writes every TypeDef field except `attributes`), `src/cli/config.rs` (add-type), `.lazyspec.toml` (fixed in working tree — inline `attributes = []` removed from bug type; commit it)
- Schema: `AttrDef`/`AttrKind` in `src/engine/config.rs:216-240`; deserialization tests `config.rs:2069-2160` show expected TOML shape
- Convention: DICTUM-004

## Tasks

1. `update_type_table`: write `attributes` as array-of-tables (toml_edit); skip when empty; remove any conflicting inline `attributes` key.
2. Round-trip test: attr-bearing TypeDef → write → reparse → equal (STORY-213 AC3).
3. add-type: attribute input (match existing add-type arg style; JSON arg acceptable).
4. Regression test: c83bb99 shape (inline `attributes = []` + `[[types.attributes]]`) → actionable error naming type via load/`fix --config` (STORY-213 AC4).
5. Commit working-tree `.lazyspec.toml` fix w/ this iteration's changes.
6. `cargo test`.

## Out of scope

Attribute semantics/validation (done — ITERATION-205). TUI attribute editing.

## Verification

`lazyspec config add-type` w/ attrs → `lazyspec validate` clean; hand-broken config → error names `bug` type, no stack trace.

