---
title: Project board store and membership relation
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

RFC-050 turns the `github-issues` store from inert storage into a native-binding layer over GitHub. This slice lands the project-board half of that layer: the `github-projects` store backend and the `membership` native-backed relation, building on the GraphQL trait from STORY-155. STORY-156's attribute round-trip is a soft prerequisite only for persisting any attributes a project doc itself carries; the membership mutations here are self-contained and do not use the `--attr` path (per-board field attributes are STORY-162).

Teams organize work on GitHub Projects v2 boards, but a lazyspec doc backed by an issue is blind to which boards it belongs to. A project document should bind to an existing board, and an issue-doc should be able to join one or more boards. Because a doc can live on many boards, board membership is modelled as a many-to-many native relation rather than a single field — each membership relation is one board the doc is an item of.

Boards are read and associated, never created from lazyspec (an explicit RFC non-goal): authoring a board is a deliberate human act on GitHub, and the board owns its own field schema. This slice deliberately stops at membership. Per-board field values (`PROJECT-n.<field>`) are STORY-162 and depend on this one; board Status is just a namespaced attribute handled there, and the board's Status never drives lazyspec lifecycle (open/closed lifecycle stays intact).

## Acceptance Criteria

- **Given** a project document configured with the `github-projects` backend and a board number `N`, and an existing Projects v2 board `N` under the owner
  **When** the store resolves the document
  **Then** it resolves the owner (org or user login) from store config, issues the `{owner} { projectV2(number:N){id} }` GraphQL lookup (the `organization` vs `user` root chosen per owner type), and binds the document to the returned project node id, with no create mutation issued.

- **Given** a project document configured with the `github-projects` backend for a board number that does not exist under the owner
  **When** the store resolves the document
  **Then** resolution fails with a not-found error and lazyspec does NOT attempt to create a board (no `createProjectV2` or equivalent mutation is ever called by this backend).

- **Given** an issue-doc and a project doc bound to board `N`, and a `github_native="membership"` relation declared (issue-doc --membership--> PROJECT-n)
  **When** the membership relation is synced
  **Then** lazyspec calls `addProjectV2ItemById(projectId, contentId)` with the board's project node id and the issue's content id, adding the issue as an item of the board.

- **Given** an issue-doc that already holds a `membership` relation to board `N`
  **When** a second `membership` relation to a different board `M` is added
  **Then** both relations persist and the issue is added as an item to both boards via two `addProjectV2ItemById` calls (membership is many-to-many; one relation per board).

- **Given** an issue-doc that is a member of board `N` via a `membership` relation
  **When** that membership relation is removed
  **Then** lazyspec deletes the project item for that board (the issue is no longer an item of board `N`), and any membership relations to other boards are unaffected.

- **Given** any operation in this slice
  **When** it serializes its result
  **Then** the result is available via `--json`.

## Scope

### In Scope

- A new `StoreBackend` variant `github-projects` extending the enum in `src/engine/config.rs`, implementing the `DocumentStore` trait (read/associate only).
- Resolving a project document to an existing board node id via the `organization|user { projectV2(number:N){id} }` GraphQL lookup, through the `GhGraphql` seam (STORY-155).
- A `github_native="membership"` relation kind on `[[relationships]]`: issue-doc --membership--> PROJECT-n, backed by `addProjectV2ItemById(projectId, contentId)`.
- Many-to-many membership: a single doc may hold multiple `membership` relations to different boards, each synced independently.
- Removing a membership relation removes the corresponding project item.
- Materializing project docs in `.lazyspec/cache/github-projects/` like issue docs (caching the resolved board node id for offline lookups); `--json` output for all operations.
- Fakes at the `GhGraphql` seam and TDD coverage for resolve, add-member, multi-board membership, and remove-member.

### Out of Scope

- Per-board field VALUES and namespaced `PROJECT-n.<field>` attributes (STORY-162, depends on this).
- The board Status -> lifecycle inheritance (deferred in RFC-050; Status is a namespaced attribute handled in STORY-162).
- Creating, editing, or deleting project boards from lazyspec (explicit RFC non-goal).
- Reading or posting issue comments.
- Conflict detection on native writes (policy is last-write-wins + refresh).
- Milestones, sub-issues, and native issue-type (other RFC-050 slices).
