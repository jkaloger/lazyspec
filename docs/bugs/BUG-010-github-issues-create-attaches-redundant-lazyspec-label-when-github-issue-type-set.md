---
title: "GitHub-issues create attaches redundant lazyspec: label when github_issue_type set"
type: bug
status: draft
author: "Jack Kaloger"
date: 2026-07-18
tags: []
related: []
---

## Summary

When a github-issues-backed type sets `github_issue_type` in config, `lazyspec create` still attaches the default `lazyspec:{name}` label to the created issue. The native issue type already classifies the issue, so the label is redundant. Worse, on read-back it surfaces as a stray document tag.

## Reproduction

1. Config a github-issues type with `github_issue_type = "Bug"` (org-owned repo).
2. `lazyspec create <type> "..."` → issue created with native type `Bug` pushed **and** label `lazyspec:{name}` attached.
3. Fetch/materialize the issue → the `lazyspec:{name}` label is read back as a document tag.

## Expected

When `github_issue_type` is set, no `lazyspec:{name}` label is attached at creation. Only an explicitly configured `github_issue_tag` (if any) is attached — matching the read/discovery semantics.

## Actual

`create_issue` (src/engine/store_dispatch.rs:1204,1237) and the materialize path (src/engine/store_dispatch.rs:825,832) unconditionally `label_ensure` + `issue_create(..., &[type_def.github_label()])`, ignoring `github_issue_type`. `github_label()` (src/engine/config.rs:1227) never consults `github_issue_type`.

## Root cause

The write path attaches the identity label regardless of classification mode. The read path already proves the label is unneeded when a native type is set:

- `discover_issues` `(None, Some(issue_type))` searches by native type only (src/engine/issue_cache.rs:550).
- `extract_type_and_tags` `(None, Some(it))` classifies on native type, with `classifying = None` — so the stray label is not filtered from tags (src/engine/issue_body.rs:246,263).

## Fix direction

Attach labels from a classification-aware helper on `TypeDef`: when `github_issue_type` is set, attach `github_issue_tag` if configured else nothing; otherwise attach `github_label()` as today. Apply at both create sites; skip `label_ensure` when no label is attached.

