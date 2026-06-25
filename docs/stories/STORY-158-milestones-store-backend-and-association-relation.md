---
title: Milestones store backend and association relation
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

RFC-050 introduces native store backends: a lazyspec document whose home is a GitHub object that is not an issue. Milestones are the first such backend. A GitHub milestone is not an issue — it has its own REST endpoints (`GET/POST /repos/{o}/{r}/milestones`, `PATCH/DELETE .../{number}`) and fields `title`, `description`, `due_on` (ISO 8601), and `state` (open/closed). `open_issues`/`closed_issues` are read-only counts, so any "percent complete" is computed, never stored.

This slice adds a `github-milestones` `StoreBackend` variant so a milestone document maps to a GitHub milestone, and surfaces the issue→milestone link (the native `issue.milestone` field) as a relation from an issue-doc to its milestone doc. It depends on STORY-155 only for the shared GitHub-access plumbing; milestones are REST, so no GraphQL is required here. Per RFC-050 the write policy is last-write-wins + refresh.

## Acceptance Criteria

- **Given** a type configured with `store = "github-milestones"`
  **When** a milestone document is created via `create`
  **Then** a GitHub milestone is created through the REST Milestones API and the doc's id maps to the milestone number.

- **Given** an existing milestone document
  **When** its `title`, `description`, or `due_on` is updated
  **Then** the change is `PATCH`ed to the milestone and re-reads return the updated values (round-trip).

- **Given** a milestone whose GitHub `state` is `closed`
  **When** the milestone doc is loaded
  **Then** its lifecycle maps to the GitHub state: `closed` state → a closed-equivalent status, `open` state → an open-equivalent status.

- **Given** an issue-backed doc and a milestone doc, with a milestone-association relation declared on the issue-doc type's config (a native-field-backed relation over `issue.milestone`)
  **When** the issue-doc is associated to the milestone
  **Then** `PATCH issues/{n}` sets `milestone` to the milestone number and lazyspec surfaces that relation from the issue-doc to the milestone doc.

- **Given** a milestone with open and closed issue counts
  **When** the milestone doc is shown with `--json`
  **Then** a computed percent-complete is derived from `open_issues`/`closed_issues` and is not treated as a writable field.

- **Given** the milestone REST client behind a trait seam
  **When** tests run
  **Then** a fake at that seam exercises create/update/associate without hitting GitHub.

## Scope

### In Scope

- `github-milestones` `StoreBackend` variant and its `DocumentStore` implementation over the REST Milestones API.
- Create/update of milestone docs (title, description, due_on, state) with open/closed ↔ lifecycle mapping.
- Issue-doc → milestone association via the native `issue.milestone` field, surfaced as a relation.
- Computed percent-complete in `--json` (read-only).
- A trait seam for the milestone REST calls with a test fake.

### Out of Scope

- Projects, sub-issues, native issue-types (separate RFC-050 stories).
- GraphQL (milestones are REST).
- Conflict detection on writes — last-write-wins + refresh per RFC-050.
- Auto-deriving milestone membership from project boards.
