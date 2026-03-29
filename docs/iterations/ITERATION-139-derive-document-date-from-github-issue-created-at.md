---
title: Derive document date from GitHub issue created_at
type: iteration
status: accepted
author: jkaloger
date: 2026-03-28
tags: []
related:
- implements: docs/stories/STORY-101-issue-body-format-and-parsing.md
---





## Context

`parse_issue()` in `src/engine/issue_cache.rs` sets `date: Utc::now().date_naive()`, meaning every document gets today's date on each fetch. The `GhIssue` struct has `updated_at` but no `created_at`. We want `created_at`.

## Changes

1. Add `created_at: String` field to `GhIssue` struct in `src/engine/gh.rs`, request `createdAt` in the GraphQL query in `issue_cache.rs`, parse `issue.created_at` (ISO 8601) into `NaiveDate` in `parse_issue()` and use it for `DocMeta.date`, falling back to `Utc::now().date_naive()` if parsing fails. Add a unit test for `parse_issue()` with a known `created_at` that asserts the correct date.

## Test Plan

- Unit test: `parse_issue()` with a known `created_at` produces the correct date
- Integration: run `cargo run -- fetch` and verify cached docs have the issue's creation date, not today's date

## Notes

ACs addressed: Document date reflects issue creation date, not fetch time
