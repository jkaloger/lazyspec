---
title: GitHub issue-type/tag classification config schema
type: iteration
status: complete
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-191
---

## Objective

Add `github_issue_tag`/`github_issue_type` fields to `TypeDef` -- schema only, no consumer.

## Context

- Story: STORY-191
- Touch: src/engine/config.rs:239-266 (`TypeDef`)
- No resolver, no read/write logic -- STORY-192 is first consumer.

## Satisfies

STORY-191 AC1-AC5 (all -- schema-only story, single field addition, not separable).

## Tasks

1. `TypeDef`: add `github_issue_tag: Option<String>` and `github_issue_type: Option<String>`, both `#[serde(default)]`, matching existing `parent_type`/`intent` pattern in the same struct.
2. Confirm `config --json` surfaces both fields as `null` when absent, configured values when set (should fall out of `#[derive(Serialize)]` on `TypeDef` -- verify, don't assume).
3. Confirm `validate` emits no finding for either field on a non-`github-issues`-store type (no new rule to add -- absence of a check is correct per story's Validation section).

## Out of scope

- Classification/matching logic -- STORY-192.
- Discovery query changes -- STORY-193.
- Dual materialization -- STORY-194.
- Write-side `create` push -- STORY-195.
- README documentation -- STORY-196.
- Reconciling with STORY-190's `github_label`/`label_override` -- open RFC-055 decision.

## Principles/conventions

Config schema addition only (CONVENTION.md L3 engine layer, no I/O) -- no CLI/TUI surface change.

## Verification

- `[[types]]` entry with both fields set parses on any `store` value, `config --json` echoes both.
- `[[types]]` entry with neither field: `config --json` shows both as `null`, existing config parsing byte-for-byte unchanged.
- `validate --json` on a fixture with `github_issue_tag` set on a `store = "filesystem"` type: zero findings mentioning either field.

