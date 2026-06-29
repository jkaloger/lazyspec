---
title: GraphQL layer and cached schema snapshot
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

RFC-050 turns the `github-issues` store from inert storage into a native-binding layer over GitHub: Projects v2 boards, native issue-types, sub-issues, and project field values. All four of those constructs are GraphQL-only on GitHub's API, so every later native-feature story needs one shared way to issue GraphQL calls and one shared way to resolve native-field ids offline.

This slice lands that shared machinery before any feature consumes it. It adds a `GhGraphql` trait — implemented on `GhCli` by shelling to `gh api graphql` and fakeable at the same trait seam as the existing `GhIssueReader`/`GhIssueWriter`/`GhAuth` — and a cached native-field schema snapshot at `.lazyspec/cache/gh-schema.json`. The snapshot caches **ids** (org issue-type ids, project field ids, single-select option ids, iteration ids), not just display names, because every native write keys off the id (e.g. `updateIssue { issueTypeId }`, `updateProjectV2ItemFieldValue`). Landing this first unblocks stories 157, 158, 159, 160, 161, and 162.

Two operational gotchas must be surfaced where they bite: Projects mutations require the `project` token scope (`gh auth refresh -s project`), and on macOS a keyring-lookup timeout can make `gh api` silently send unauthenticated requests, worked around with `GH_TOKEN="$(gh auth token)" gh api …`.

## Acceptance Criteria

- **Given** a valid GraphQL query and a configured `gh` auth token
  **When** `GhGraphql::graphql(query, vars)` is called on the `GhCli` impl
  **Then** it shells to `gh api graphql`, uses the `gh` auth token automatically, and returns the response parsed as `serde_json::Value`.

- **Given** a test exercising engine code that depends on `GhGraphql`
  **When** the test substitutes a fake implementation of the trait at the same seam as `GhIssueReader`/`GhIssueWriter`
  **Then** the code under test runs against canned GraphQL responses with no `gh` process invoked.

- **Given** a vars argument containing both string and typed (int/bool) variables
  **When** `GhGraphql::graphql` builds the `gh api graphql` invocation
  **Then** each var is flattened into a repeated `-f key=value` (string) or `-F key=value` (typed) flag — never a single JSON variables blob.

- **Given** a github-backed store with org issue-types and project fields defined on GitHub
  **When** the store is refreshed
  **Then** the field/type schema is fetched and persisted to `.lazyspec/cache/gh-schema.json`, caching issue-type ids, project field ids, single-select option ids, and iteration ids (not only display names).

- **Given** an existing `.lazyspec/cache/gh-schema.json` snapshot and no network access
  **When** `validate` (or the store) reads native-field schema to check an attribute value
  **Then** it resolves names and ids from the snapshot offline without calling GitHub.

## Scope

### In Scope

- A `GhGraphql` trait whose `graphql(query, vars)` returns parsed `serde_json::Value`, implemented on `GhCli` via `gh api graphql`, fakeable at the trait seam.
- Flattening the vars argument into repeated `-f` (string) / `-F` (typed) flags; the trait signature reflects this rather than implying a JSON payload.
- Automatic use of the `gh auth login` token (no separate token handling).
- Fetching the native-field schema (org issue-types, project field options/iterations) on store refresh and persisting it to `.lazyspec/cache/gh-schema.json`, caching ids.
- Offline read of the snapshot by `validate` and the store.
- Documenting the two operational gotchas: `project` scope (`gh auth refresh -s project`) and the macOS keyring `GH_TOKEN="$(gh auth token)"` workaround.

### Out of Scope

- Any specific native feature that consumes this layer — native issue-type as attribute, milestones, sub-issues, comments read-thru, project membership/board store, and per-board field attributes are all later stories (157–162).
- The attribute write path and github attribute round-trip (separate foundational story).
- A direct GraphQL HTTP client (e.g. octocrab); all access stays behind the `gh` CLI seam.
- Conflict detection on native writes; policy remains last-write-wins + refresh.
- Cache TTL / staleness handling beyond writing and reading the snapshot.
