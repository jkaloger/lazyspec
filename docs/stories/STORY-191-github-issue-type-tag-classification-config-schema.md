---
title: GitHub issue-type/tag classification config schema
type: story
status: complete
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-055
- blocks: STORY-192
---## Problem

RFC-055 makes native GitHub Issue Type a classification signal for `github-issues`-store types, and folds STORY-190's proposed `github_label` field into a renamed `github_issue_tag`. Neither field exists on `TypeDef` yet (src/engine/config.rs:239-266): today a `[[types]]` entry has no way to declare a native issue-type match or a tag-based match independent of the `lazyspec:{type}` prefix scheme.

## Goal

Add the two config fields RFC-055's Design section specifies, parseable and round-tripping through `config --json`, with no behavior change — no classification, discovery, or write-side logic consumes them yet. That is stories 2-5 (below); this story is the schema foundation they all build on.

## Design

Add to `TypeDef` (src/engine/config.rs:239-266):

```rust
pub struct TypeDef {
    // ...existing fields...
    #[serde(default)]
    pub github_issue_tag: Option<String>,
    #[serde(default)]
    pub github_issue_type: Option<String>,
}
```

Both default to `None` when absent from TOML, matching the `#[serde(default)]` pattern already used for `parent_type` and `intent` (also `Option<String>`) in the same struct, and per STORY-190's identical treatment of `github_label`.

### Validation — inert on non-`github-issues` types

Per STORY-190's precedent (docs/stories/STORY-190-custom-github-label-per-document-type.md, "Validation" section) for its `github_label` field: a `github_issue_tag` or `github_issue_type` set on a type whose `store` is not `github-issues` is unused by any store and should be silently ignored by `validate`, not flagged as an error or warning. Same reasoning applies here — this is a config directive with no data-integrity implication, consistent with how `agents` is unused for non-iteration types today. No new validation rule is added; the absence of a check is the intended behavior.

No resolver method (cf. STORY-190's `github_label()`) is needed yet — nothing reads these fields in this story. STORY-192 (per-type match-rule plumbing) is where a consuming type first reads `github_issue_tag`/`github_issue_type` off `TypeDef`.

## Non-goals

- Classification/matching logic (read side) — STORY-192 and STORY-193.
- Dual materialization when a type's rule matches more than one type — STORY-194.
- Write-side behavior (`create` pushing native issue type, tagging with `github_issue_tag`) — STORY-195.
- README documentation of the new fields — STORY-196.
- Reconciling `github_issue_tag` with STORY-190's `github_label` field (RFC-055 flags this as an open Decision, not resolved by this story).

## Acceptance criteria

- A `[[types]]` entry with `github_issue_tag = "some-tag"` and/or `github_issue_type = "Feature"` parses without error, on any `store` backend.
- `config --json` output for that type includes `github_issue_tag` and `github_issue_type` with the configured values.
- Omitting both fields keeps existing config parsing behavior unchanged (regression-free default; both surface as `null` in `config --json`).
- `validate` does not report any finding — error or warning — for either field set on a non-`github-issues`-store type.
- Setting either field has no effect on fetch, create, update, or delete behavior (no consumer exists yet).
