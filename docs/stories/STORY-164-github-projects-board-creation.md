---
title: "github-projects board creation"
type: story
status: complete
author: "jkaloger"
date: 2026-06-26
tags: []
related:
- implements: RFC-050
---
## Context

STORY-161 bound a `github-projects` document to an **existing** Projects v2 board, read/associate only: `resolve_board` looks up `projectV2(number: N)` and `create`/`delete` bail so boards are never authored from lazyspec. That decision kept the first slice small, but it has a sharp edge surfaced during dogfooding: a project document's identity is its board number (`PROJECT-n`, parsed by `board_number`), so a project doc can only exist if someone first creates the board in the GitHub UI and then authors a doc whose number happens to match. There is no path from `lazyspec create project` to a working board.

This slice closes that gap by letting `create` author the board. The github-projects store's `create` calls `createProjectV2(input: { ownerId, title })` over the `GhGraphql` seam, then persists the returned board number as the document id so subsequent `PROJECT-n` membership and field writes resolve against it. `ownerId` requires resolving the owner node id, which is an org-or-user lookup and therefore depends on STORY-165's resolution helper. `delete` remains a bail (destroying a board is out of scope); unbinding a doc without destroying the board stays a separate concern.

Board creation needs the `project` token scope, which `repo` does not grant. The store must fail with an actionable error ("Projects v2 board creation needs `gh auth refresh -s project`") rather than a raw GraphQL permission error, mirroring how the schema-snapshot path already names the missing scope.

## Acceptance Criteria

- **Given** a `github-projects` type and a token with the `project` scope
  **When** `create project "<title>"` runs
  **Then** `createProjectV2` is issued via `GhGraphql`, a board is created under the resolved owner, and the document id is set to the returned board number so `PROJECT-n` resolves.

- **Given** a token missing the `project` scope
  **When** `create project` runs
  **Then** the store returns an error naming the required scope and the `gh auth refresh -s project` remedy, and no document is persisted.

- **Given** a project document created by this slice
  **When** an issue doc is linked to it via the `membership` relation (STORY-161)
  **Then** the membership write resolves the board node id from the freshly created board with no manual UI step.

- **Given** an owner that is a user account and an owner that is an organization
  **When** `create project` resolves the owner node id
  **Then** both resolve correctly through the STORY-165 helper (no org-only assumption).

## Scope

### In Scope

- Implement `create` on the github-projects store via `createProjectV2`, persisting the returned board number as the doc id.
- Resolve the owner node id (org-or-user) for `ownerId`, reusing the STORY-165 resolver.
- Scope-missing detection with an actionable error message.
- Fakes at the `GhGraphql` seam covering create-success, owner resolution, and the scope-missing path; `--json` preserved on `create`.

### Out of Scope

- `delete` of a board (stays a bail); unbinding a doc from a board without destroying it.
- Editing board metadata after creation (title/description/views).
- Per-board field schema authoring — fields are read/associate per STORY-162, not created here.
- Token-scope acquisition automation; the user runs `gh auth refresh` themselves.
