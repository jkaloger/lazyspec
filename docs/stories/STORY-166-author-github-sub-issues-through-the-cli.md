---
title: "Author github sub-issues through the CLI"
type: story
status: complete
author: "jkaloger"
date: 2026-06-26
tags: []
related:
- implements: RFC-050
---
## Context

STORY-159 wired native sub-issues: a subdirectory parent's `index.md` becomes the parent issue and each child `.md` becomes a sub-issue, reconciled over GraphQL by `reconcile_subissues`. The engine path is sound, but there is no CLI route to produce the subdirectory shape it consumes. `create` always emits a flat document; the integration tests build subdirectory fixtures by writing child files directly (`write_child_doc`). For filesystem types a user can `mkdir` and drop files, but `github-issues` documents live only under `.lazyspec/cache/<type>/` and are managed by the tool, so there is no authored source tree to hand-edit. The net effect: sub-issues are reachable from test fixtures and never from the shipped CLI.

This slice adds the missing authoring command. `create <type> "<title>" --parent <PARENT-ID>` creates a child document inside the parent's subdirectory (promoting a flat parent to the `TYPE-n-slug/index.md` form on first child), so the sub-issue reconciliation that already exists has a subdirectory to act on. For `github-issues` types this materializes the child as its own issue and links it under the parent's native sub-issues on the next fetch; for filesystem types it produces the same `children_of`/`parent_of` structure the loader already tracks. The command is store-agnostic — it manipulates document structure, and the existing store dispatch decides what native binding follows.

Two smaller defects observed in the same area are folded in only as regression guards, not features: `list <type> --json` reports `"id": null` for github-backed docs (the id lives in the path but is not surfaced on the record), and a transient empty-stem `.md` file was observed mid-fetch in the cache directory before self-healing. Both are noise that obscures the sub-issue authoring flow.

## Acceptance Criteria

- **Given** a flat parent document and a child type
  **When** `create <type> "<title>" --parent <PARENT-ID>` runs
  **Then** the parent is promoted to `TYPE-n-slug/index.md` (if not already a subdirectory) and the child is created as a sibling `.md`, tracked by the loader's `children_of`/`parent_of`.

- **Given** a `github-issues` parent and a child created via `--parent`
  **When** the next fetch reconciles sub-issues
  **Then** the child materializes as its own issue and appears under the parent's native sub-issues (end-to-end coverage of the STORY-159 path from a CLI-authored child).

- **Given** a `--parent` whose store differs from the child type's store
  **When** `create --parent` runs
  **Then** the same-store guard rejects it before any file or remote mutation, consistent with `reconcile_subissues`.

- **Given** any `github-issues` document
  **When** `list <type> --json` is run
  **Then** each record carries its document id (no `null` id).

## Scope

### In Scope

- A `--parent <ID>` option on `create` that places the new document in the parent's subdirectory, promoting a flat parent to `index.md` form as needed.
- Same-store guard at the CLI boundary mirroring `reconcile_subissues`.
- Surface the document id on `list --json` records for github-backed types (regression guard).
- Investigate and eliminate the transient empty-stem `.md` write in the github cache path (regression guard).
- `--json` preserved on `create` and `list`.

### Out of Scope

- A separate top-level `child` subcommand (folded into `create --parent`).
- Deep nesting policy beyond what STORY-159 already supports (flat parent→child within GitHub's limits).
- Re-parenting or moving existing children between parents.
- Changing sub-issue reconciliation semantics (this slice only feeds it a subdirectory).
