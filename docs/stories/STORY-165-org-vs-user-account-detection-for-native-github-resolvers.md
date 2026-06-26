---
title: "Org-vs-user account detection for native github resolvers"
type: story
status: draft
author: "jkaloger"
date: 2026-06-26
tags: []
related:
- implements: RFC-050
---
## Context

The native-binding resolvers split on a hidden assumption: an owner is an organization. Two GraphQL queries in `src/engine/gh_schema.rs` hardcode `organization(login: $owner)` with no fallback — `ISSUE_TYPES_QUERY` (`:104`) and `PROJECT_FIELDS_QUERY` (`:106`) — and parse their responses through the `/data/organization/...` pointer (`:152`, `:177`). When the owner is a personal account, GitHub returns "Could not resolve to an Organization with the login of '<owner>'" and the schema snapshot at `.lazyspec/cache/gh-schema.json` never populates. Dogfooding against `jkaloger/lazyspec` (a user repo) reproduced exactly this on every `fetch`.

The codebase already contains the pattern for getting this right: `resolve_project_id_live` in `store_dispatch.rs` tries `organization(login:)` first and falls back to `user(login:)`. The schema-snapshot fetch predates that helper and was never updated. This slice extracts the org-then-user resolution into one place and routes all native-field queries through it, so personal-account repos populate the snapshot and resolve project boards identically to org repos.

`issue_type` (native `issueType`) is a separate matter: GitHub exposes custom issue types only at the organization level, so on a user account there is nothing to bind. The correct behaviour is not an error but graceful absence — the snapshot records an empty issue-type set, `issue_type` is absent from attributes, and any `--attr issue_type=...` write fails with a message that names the org-only constraint rather than a raw GraphQL error.

## Acceptance Criteria

- **Given** an owner that is a personal/user account
  **When** the schema snapshot is refreshed during `fetch`
  **Then** the project-fields query resolves via the `user(login:)` fallback, the snapshot populates, and no "Could not resolve to an Organization" warning is emitted.

- **Given** an owner that is an organization
  **When** the schema snapshot is refreshed
  **Then** resolution uses the `organization(login:)` path and behaves exactly as before (regression coverage).

- **Given** a personal-account repo with no custom issue types
  **When** the snapshot is refreshed and an issue is read
  **Then** the issue-type set is empty, `issue_type` is absent from the document attributes, and no warning treats the empty set as a failure.

- **Given** a personal-account repo
  **When** `update --attr issue_type=<value>` is attempted
  **Then** the write fails with an error stating that native issue types require an organization, before any mutation is issued.

## Scope

### In Scope

- A single org-then-user owner-resolution helper, reused by the schema-snapshot queries (`ISSUE_TYPES_QUERY`, `PROJECT_FIELDS_QUERY`) and by `resolve_project_id_live`.
- Response parsing that reads whichever of `organization`/`user` the query resolved against.
- Graceful handling of an absent issue-type set on user accounts (empty snapshot, attribute absent, actionable write error).
- Fakes at the `GhGraphql` seam exercising both the org path and the user-fallback path for each query.

### Out of Scope

- Caching the org-vs-user classification across runs (re-resolve per refresh is acceptable).
- Creating organizations or migrating a repo's owner type.
- The Projects v2 board-creation path itself (STORY-164 consumes this helper).
- Surfacing the snapshot warnings in the TUI (STORY-163 owns the routing).
