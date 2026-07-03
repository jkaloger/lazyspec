---
title: Custom github_label field per document type
type: iteration
status: complete
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-190
---

## Objective

`github_label` field on `TypeDef` -- custom label per type, default `lazyspec:{type}` unchanged.

## Context

- Story: STORY-190
- Touch: src/engine/config.rs:239-266 (`TypeDef` + resolver), src/engine/gh.rs:218-220 (`type_label`), src/engine/issue_body.rs:169-190 (`extract_type_and_tags`), src/engine/issue_cache.rs (:161,:201,:313,:352,:553,:599), src/cli/init.rs:85, src/engine/store_dispatch.rs (:268-282,:495,:897,:955-969,:1092-1106,:1131), README.md:536+
- Design detail (resolver name clash, plumbing shape, validation stance): STORY-190 body verbatim -- don't re-derive, follow as written.

## Satisfies

STORY-190 AC1-AC4 (all -- single plumbing pass, not separable: write-side swap and read-side exact-match fix share one `label` field threaded through the same call chain).

## Tasks

1. `TypeDef`: add `label_override: Option<String>` (`#[serde(default)]`), add `github_label()` resolver (falls back `gh::type_label(&self.name)`).
2. Write side: swap all 6 `gh::type_label(&type_def.name)` call sites (init.rs:85, store_dispatch.rs:495/897/1131, issue_cache.rs:161/313) for `type_def.github_label()`.
3. Read side: introduce per-type label data threaded through `parse_issue` -> `IssueContext` -> `deserialize` -> `extract_type_and_tags` (replace bare `known_types: &[String]`/`&[&str]` with resolved label per type). Fix exact-match against resolved label, not `lazyspec:` prefix strip. Also fix fallback path issue_cache.rs:599 (`.starts_with("lazyspec:")`).
4. Validation: no new rule -- `github_label` on non-`github-issues` type stays silently unused (confirm, don't add a check).
5. README.md:536+: document `github_label` field, same style as sibling `[[types]]` fields in that section.
6. Update existing unit tests touching `known_types` construction at listed call sites for the new shape.

## Out of scope

- `github_issue_tag`/`github_issue_type` fields -- STORY-191/192, separate mechanism.
- Custom label colors/descriptions -- unchanged (`deterministic_color`, fixed description string).
- `github-milestones` store -- no label scheme there, untouched.
- Reconciling `github_label` vs STORY-191's `github_issue_tag` overlap -- open RFC-055 decision, not this slice.

## Principles/conventions

Engine change only where read/write call sites live (layering per CONVENTION.md L3) -- CLI/TUI untouched, no surface change beyond config field + README.

## Verification

- `[[types]] github_label = "Ticket"` type: create/fetch/delete issue -> label is literally `Ticket`, never `lazyspec:ticket`.
- Type with `github_label` unset: byte-for-byte same label as before this change.
- Round-trip: issue carrying custom label only (no `lazyspec:` prefix at all) -> `extract_type_and_tags` still resolves correct type on fetch.

