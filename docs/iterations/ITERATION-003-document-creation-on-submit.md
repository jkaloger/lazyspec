---
title: Document Creation on Submit
type: iteration
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
- creation
- engine
related:
- implements: docs/stories/STORY-007-document-creation-on-submit.md
---


## Changes

- Add `submit_create_form(root, config)` method on `App` that validates, creates file, applies tags/relations, reloads store, and navigates to new doc
- Add `parse_relations()` helper that resolves shorthand (e.g. `RFC-001`) via `store.resolve_shorthand()` and parses optional type prefix (`implements:`, `supersedes:`, `blocks:`, `related-to:`)
- Add `update_tags()` free function that writes proper YAML sequences into frontmatter (avoids string-vs-sequence issue with `cli::update::run`)
- Wire `Enter` in create form mode in `tui/mod.rs` event loop to call `submit_create_form`
- Validation: empty title sets error and keeps form open; unresolvable relation shorthand sets error and keeps form open

## Test Plan

- `test_submit_creates_document` (AC1)
- `test_submit_creates_correct_type` (AC1)
- `test_submit_applies_tags` (AC2)
- `test_submit_applies_relations` (AC3)
- `test_submit_relation_defaults_to_related_to` (AC4)
- `test_submit_empty_title_shows_error` (AC5)
- `test_submit_invalid_relation_shows_error` (AC6)
- `test_submit_navigates_to_new_doc` (AC7)

## Notes

AC8 (file watcher pickup) requires no new code. The existing `notify` watcher in `tui/mod.rs` already watches all document directories and calls `store.reload_file()` on changes. The `submit_create_form` method also calls `store.reload_file()` directly for immediate feedback, so the user sees the doc without waiting for the watcher.

`cli::update::run` writes all values as YAML strings, which breaks tag arrays. The `update_tags` free function manipulates the YAML value tree directly to produce a proper sequence.
