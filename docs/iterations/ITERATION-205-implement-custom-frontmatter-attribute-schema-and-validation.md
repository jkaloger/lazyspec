---
title: Implement custom frontmatter attribute schema and validation
type: iteration
status: draft
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: STORY-150
---<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Changes

- `src/engine/config.rs` (~`:195` `TypeDef`): add `attributes: Vec<AttrDef>`, `#[serde(default)]`.
  - New `struct AttrDef { name: String, kind: AttrKind, #[serde(default)] required: bool, #[serde(default)] values: Vec<String> }`.
  - New `enum AttrKind { Int, Float, Str, Enum, Date, Bool }`, serde rename lowercase.
- `src/engine/document.rs`:
  - New `enum AttrValue { Int(i64), Float(f64), Str(String), Bool(bool), Date(NaiveDate), Raw(serde_yaml::Value) }`. `Raw` = undeclared keys.
  - `RawFrontmatter` (`:197`): capture leftover keys via `#[serde(flatten)] extra: BTreeMap<String, serde_yaml::Value>`.
  - `DocMeta` (`:182`): add `attributes: BTreeMap<String, AttrValue>`. `parse()` (`:285`) coerce extra keys against type schema -> typed `AttrValue`, undeclared -> `Raw`. Date reuse `deserialize_naive_date` (`:11`).
- Validation: where validate rules run (engine validate module). Per doc, per declared attr:
  - wrong kind / enum not in `values` / missing `required` -> error.
  - undeclared key -> warning.
  - Schema lookup = doc type's `TypeDef.attributes`.

## Test Plan

- AC1: config w/ `[[types.attributes]]` deserializes -> `TypeDef.attributes` populated, all 6 kinds parse. unit.
- AC2: doc frontmatter `estimate: 5` -> `DocMeta.attributes["estimate"] == Int(5)`; `date` attr -> `Date`. unit.
- AC3: wrong kind / bad enum / missing required each -> validate **error**. 3 cases.
- AC4: undeclared key -> validate **warning**, doc still parses. unit.

## Notes

- `AttrValue::Raw` resolves RFC-049 review gap (undeclared keys need a home).
- Additive to `DocMeta` (hot struct) — keep map empty-default so existing parse paths unchanged.
- No TUI/CLI surfacing here (ITERATION-207).
