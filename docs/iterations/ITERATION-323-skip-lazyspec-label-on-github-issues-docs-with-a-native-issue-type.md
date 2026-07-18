---
title: 'Skip lazyspec: label on github-issues docs with a native issue type'
type: iteration
status: draft
author: Jack Kaloger
date: 2026-07-18
tags: []
related:
- implements: STORY-195
- related-to: BUG-010
---

## Objective

Stop attaching the `lazyspec:{name}` label to github-issues documents whose type sets `github_issue_type`; the native issue type already classifies them.

## Context

- Story: STORY-195 (push native issue type on create)
- Bug: BUG-010 (repro, root cause, read-path proof)
- Match/discovery semantics governing the label decision: src/engine/issue_cache.rs `discover_issues`, src/engine/issue_body.rs `extract_type_and_tags`
- Touch: src/engine/config.rs (new `TypeDef` label helper next to `github_label`), src/engine/store_dispatch.rs (both create sites: `materialize` ~825-832, `create_issue` ~1204-1237)

## Satisfies

BUG-010. Extends STORY-195's create path.

## Tasks

1. Test-first: unit test in store_dispatch tests asserting a type with `github_issue_type` set and no `github_issue_tag` creates an issue with **no labels** (assert `last_create_labels` empty), native type still pushed; and a type with both set attaches only the tag.
2. Add `TypeDef::github_create_labels() -> Vec<String>` in src/engine/config.rs: `github_issue_type.is_some()` → `github_issue_tag` if set else empty; else `vec![github_label()]`. Unit-test the helper's three cases.
3. Replace `let label = type_def.github_label(); ... &[label]` at both create sites with the helper: `label_ensure` per attached label (skip when empty), pass the vec to `issue_create`.
4. Fix the stale doc comments at src/engine/config.rs:273-282 that claim `github_issue_tag`/`github_issue_type` are "schema only — no logic reads them" (they now drive discovery, classification, and this write path).

## Out of scope

- The `github_issue_type`-unset path stays byte-for-byte identical; the tag-vs-label question for non-typed types is not touched here.
- TUI/web/CLI surfaces — this is engine write-path only; no interface change.
- BUG-002 status-seed quirk (separate bug).

## Principles/conventions

- CONVENTION (engine owns core logic, no I/O in engine; fakes only at trait seams).
- Rust idioms; add the helper on `TypeDef` (one home for the label decision, mirrors `github_label`).

## Verification

- `github_issue_type` set, no tag → created issue carries zero labels; round-trip fetch yields no stray tag.
- `github_issue_type` + `github_issue_tag` set → only the tag label attached.

