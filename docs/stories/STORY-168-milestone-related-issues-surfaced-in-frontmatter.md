---
title: Milestone related-issues surfaced in frontmatter
type: story
status: accepted
author: unknown
date: 2026-06-26
tags: []
related:
- implements: RFC-050
---

## Context

The `github-milestones` store binds milestone documents read-only. The native relation `targets` (github_native: `milestone`, inverse `targeted-by`) is read in one direction: an issue document records which milestone it `targets`. The milestone document itself does not surface the inverse — the set of issues that target it — in its `related` frontmatter.

## Goal

When a milestone is resolved, populate its `related` frontmatter with one `targeted-by` entry per issue that targets it, so the milestone shows its member issues as related in `show`, `status --json`, and the TUI graph's `related` column.

## Acceptance criteria

- A milestone document's `related` list contains a `targeted-by` entry for every open and closed issue assigned to that milestone on GitHub.
- The entries reference issues by lazyspec shorthand ID (e.g. TICKET-n / STORY-n), not raw issue numbers or cache paths.
- The inverse is consistent with existing forward `targets` reads: if issue X `targets` milestone M, then M lists X as `targeted-by`.
- `lazyspec validate --json` reports no dangling-relation findings for the backfilled entries.
- Read-only: no writes to GitHub milestones; the `related` view is derived at resolve time.

## Non-goals

- Project board `has-member` inverse (separate work).
- Writing or reconciling native relations back to GitHub.
