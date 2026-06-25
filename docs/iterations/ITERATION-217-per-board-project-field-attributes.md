---
title: Per-board project field attributes
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-162
---## Changes

Hardest slice. Depends 216 (`github-projects` store + `membership` relation + cached project node ids). All edits in github seam + dynamic-attr surface.

### 1. Read field values -> `PROJECT-n.<field>` namespaced dynamic attrs

- `src/engine/gh.rs`: extend `GhGraphql` trait (210) with `project_item_fields(repo, content_node_id) -> Result<Vec<ProjectFieldValue>>` shelling `gh api graphql` over `...items(...){fieldValues{...on ProjectV2ItemFieldSingleSelectValue{...} ...Number/Date/Text/Iteration}}`. New struct `ProjectFieldValue { project_number: u64, field_name: String, kind: GhFieldKind, value: GhFieldValueRepr }`. `enum GhFieldKind { SingleSelect, Iteration, Number, Date, Text }`.
- `src/engine/store_dispatch.rs` `GithubIssuesStore::read`/load path: for each board the issue-doc is a member of (membership relation from 216 -> project number N + cached node id), call `project_item_fields`, then inject into `DocMeta.attributes` (`BTreeMap<String, AttrValue>`, document.rs:270) keyed `format!("PROJECT-{}.{}", n, field_name)`.
- Field-type -> `AttrValue` (document.rs:186) mapping fn `gh_field_to_attr(kind, repr) -> AttrValue`: SingleSelect->`Str` (enum-as-string; option name), Iteration->`Str` (title), Number->`Int` if integral else `Float`, Date->`Date` (NaiveDate), Text->`Str`. Dynamic => carried as coerced `AttrValue`, NOT `AttrValue::Raw`, NOT a declared `AttrDef`. NB: RFC says single-select->"enum" but NO `AttrValue::Enum` variant exists (only `AttrKind::Enum`, config-side) => single-select maps to `AttrValue::Str` (option name).

### 2. Board-id namespacing -> no collision

- Key prefix `PROJECT-{n}.` derived from project number, applied at injection (step 1). `Status` on board 1 vs 2 -> `PROJECT-1.Status` / `PROJECT-2.Status`, distinct `BTreeMap` keys => structurally cannot collide. No per-name dedup needed; namespace is the discriminator.

### 3. Write path (`--attr PROJECT-n.<field>=<value>`) -- THREE-step id resolution

- `src/cli/update.rs` already routes `--attr key=value` (211) into store. `store_dispatch.rs`: detect key matching `^PROJECT-(\d+)\.(.+)$` -> branch to `set_project_field(project_number, field_name, value)` instead of HTML-comment round-trip (211).
- New `src/engine/gh.rs` `GhGraphql` mutations:
  - Step 1 resolve project node id: `organization|user { projectV2(number:N){id} }` -- reuse id cached by 216 membership binding (`issue_map`/board cache); fresh lookup only if absent.
  - Step 2 resolve field id + (SingleSelect/Iteration) option/iteration id FROM `gh-schema.json` snapshot (210) -- ids not names.
  - Step 3 `update_project_v2_item_field_value(project_id, item_id, field_id, value: GhFieldValueInput)` -> `updateProjectV2ItemFieldValue(input:{...value:{<one-key>}})`.
- `enum GhFieldValueInput` serializes to a `value` object with EXACTLY ONE key: `SingleSelect(option_id)`->`{singleSelectOptionId}`, `Iteration(iter_id)`->`{iterationId}`, `Number(f64)`->`{number}`, `Date(NaiveDate)`->`{date}`, `Text(String)`->`{text}`. Custom `Serialize` emits single key only -- extra null keys rejected by GitHub.

### 4. Clear -- DISTINCT mutation

- Empty value / unset for a set field -> `clear_project_field` -> `clearProjectV2ItemFieldValue(input:{projectId,itemId,fieldId})`. Separate `GhGraphql` method, separate code branch in `set_project_field`. Never an empty-string `text` write (rejected). Set and clear are distinct mutations.

### 5. Offline snapshot validation -- reject unknown option

- `src/engine/validation.rs`: dynamic `PROJECT-n.<field>` attrs bypass declared-`AttrDef` loop (validation.rs:1001) and currently fall to undeclared-key warning (validation.rs:1036). Add snapshot-backed check: load `.lazyspec/cache/gh-schema.json` (210), for each `PROJECT-n.<field>` attr resolve board n -> field -> allowed option/iteration set; if SingleSelect/Iteration value not in snapshot option set => `Error` (new `ValidationIssue::UnknownProjectFieldOption{path,attr,allowed}`). Runs BEFORE any mutation (write path also calls resolver in step 2 -> id-not-found = same offline reject).
- Number/Date/Text: type-shape check vs snapshot field kind only.

## Test Plan

- AC1 (field values surface): fake `GhGraphql` returns SingleSelect+Number+Date+Text+Iteration values for a member issue-doc; load doc; assert `attributes["PROJECT-1.Status"]==AttrValue::Str("In Progress")`, `PROJECT-1.Estimate`==`Int`/`Float`, `PROJECT-1.Due`==`Date`, text->`Str`, iteration->`Str`.
- AC1 (type mapping): unit-test `gh_field_to_attr` each `GhFieldKind` -> correct `AttrValue` variant; integral number->`Int`, fractional->`Float`.
- AC2 (namespacing no collision): doc member of board 1 and 2 both with field `Status`; assert keys `PROJECT-1.Status` and `PROJECT-2.Status` both present, distinct values, neither overwrites.
- AC3 (single-select write = 3 ids + one key): `--attr PROJECT-1.Status="In Progress"`; assert fake records (a) project node-id resolve, (b) field-id+option-id resolve from snapshot, (c) `updateProjectV2ItemFieldValue` value object serialized = exactly `{singleSelectOptionId:<id>}` (no other keys).
- AC4 (iteration write): `--attr PROJECT-1.Sprint=<iter>`; assert value object = exactly `{iterationId:<id>}`.
- AC5 (clear != empty string): set single-select then clear attr; assert `clearProjectV2ItemFieldValue` invoked (not `updateProjectV2ItemFieldValue` with empty text).
- AC6 (snapshot rejects unknown option): snapshot lacks option "Frozen"; `--attr PROJECT-1.Status=Frozen` and `validate`; assert offline `Error` (UnknownProjectFieldOption), NO mutation attempted (fake records zero writes).
- AC7 (number/date/text single key): write each; assert value objects = exactly `{number}` / `{date}` / `{text}` respectively, single key each.

## Notes

- HARDEST slice in RFC-050; isolated from membership (216) deliberately.
- `value` object EXACTLY ONE key -- extra null keys rejected by GitHub => `GhFieldValueInput` custom `Serialize` must emit single key, never null siblings.
- Clear != empty string: `clearProjectV2ItemFieldValue` is a DISTINCT mutation; empty-string `text` write is rejected.
- IDS not names everywhere: project node id, field id, single-select option id, iteration id all resolved (snapshot or live) before mutation.
- Dynamic attrs bypass config `AttrDef` (validation.rs:1001 loop) => no static guarantees; `gh-schema.json` snapshot is the compensating control (RFC-050 Risks). Offline validation best-effort; stale/removed option still fails at GraphQL mutation = backstop.
- Write policy last-write-wins + refresh (RFC-050); no conflict detection here.
- Depends 216 (board store + membership + cached project node ids), 211 (attr write path + `--attr`), 210 (`GhGraphql` + snapshot).