---
title: Decode ClickUp custom-field relations on read
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-200
- blocks: ITERATION-278
---

## Objective
ClickUp task relation data held in a configured custom field decodes into DocMeta relations so `context --json` resolves them like fs docs; add `clickup_custom_field_map` resolver (name/id) covering relation types + non-native attrs.

## Context
- Story+AC: STORY-200 (AC2, AC3)
- Design: RFC-056 §Relations, §Field mapping, §Config (clickup_custom_field_map)
- Base read path: STORY-198 (ClickupTasksStore read, TaskMap, cache materialize, native priority/estimate/due) already lands — do not touch
- Touch: config.rs (TypeDef.clickup_custom_field_map + config_write round-trip), ClickupTasksStore materialize path (decode custom-field payload -> DocMeta.related + non-native attrs)

## Satisfies
STORY-200 AC2, AC3. AC1 (write via link) deferred -> next iter.

## Tasks
1. add TypeDef.clickup_custom_field_map (relation/attr name -> ClickUp custom field id) + config_write round-trip, per RFC §Config.
2. resolver: name-or-id -> custom field id (fwd for write, reverse for decode); serve both relations + non-native attrs.
3. test-first: fetched task carrying serialized relation payload (issue_body.rs YAML relations-block format, e.g. `- implements: RFC-056`) in the configured text field -> DocMeta.related resolves; non-native attr resolves by name and by id.
4. wire decode into materialize so cache doc carries relations + non-native attrs.

## Out of scope
- write/persist path (`link`) -> next iter (AC1)
- ClickUp native dependency/linked-task API (RFC non-goal)
- native fields priority/estimate/due (STORY-198)
- generalizing github_native path (RFC non-goal)

## Principles
- CLAUDE.md: dogfood cargo run, --json, keep tui/web/cli in sync
- RFC-056 decisions: relations via custom field, not native
- testing skill: test-first

## Verification
context --json on ClickUp doc whose configured relation field holds implements:RFC-056 -> related[] matches fs-doc shape.

## Acceptance
Given ClickUp doc whose configured relation custom field holds implements=RFC-056, When context --json, Then related contains {type:implements,target:RFC-056}, identical shape to a filesystem doc.
Given clickup_custom_field_map maps a relation name and an attr name to field ids, When resolver runs, Then both resolve by name and by id.
