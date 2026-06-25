---
title: Native issue-type as document attribute
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

GitHub supports org-level **native issue-types** (Bug/Task/Feature plus org-defined custom types), GA since 2025-03-17 — the old `issue_types` preview header is no longer required. These are orthogonal to lazyspec's own document type, which is and stays the `lazyspec:{type}` label: an issue can be a lazyspec `story` *and* a GitHub `Bug` simultaneously.

Today the github store is blind to native issue-types. A doc backed by a GitHub issue cannot read its issue-type, and there is no way to set one from lazyspec. This slice surfaces the native issue-type as a single document attribute, `issue_type`, synced bidirectionally, so teams that organize work by GitHub issue-type keep that state visible to and editable from lazyspec.

This is a vertical slice of RFC-050's native-binding layer (shape 3, "native attributes"). It builds directly on the two foundational stories: the `GhGraphql` trait and the org issue-type **ids** cached in the schema snapshot (STORY-155), and the `--attr` write path plus github attribute round-trip (STORY-156). Because there is no dedicated set-type GraphQL mutation, the write rides the `updateIssue` mutation with `issueTypeId`, and the id must be resolved from the cached snapshot — which is why the snapshot caches ids, not just display names. Validation reads that same snapshot, so an invalid `issue_type` is rejected offline before any mutation is attempted.

## Acceptance Criteria

- **Given** a GitHub issue whose native issueType is `Bug`
  **When** the doc is fetched into the cache
  **Then** the document carries an `issue_type` attribute with value `Bug`, sourced from the issue's `issueType` field.

- **Given** an issue-backed doc with no native issue-type set
  **When** the doc is fetched
  **Then** the `issue_type` attribute is absent (or null), not an empty string or a fabricated default.

- **Given** the org schema snapshot contains issue-type `Bug` with a resolved id
  **When** `lazyspec update <id> --attr issue_type=Bug` is run
  **Then** exactly one `updateIssue` mutation is issued carrying `issueTypeId` set to the id resolved from the snapshot for `Bug`.

- **Given** an issue-backed doc that currently has `issue_type=Bug`
  **When** `lazyspec update <id> --attr issue_type=` (empty) is run to clear it
  **Then** the `updateIssue` mutation is issued with `issueTypeId` set to `null`.

- **Given** an `issue_type` value that does not appear in the org issue-type list in the snapshot
  **When** `lazyspec update <id> --attr issue_type=Nonsense` is run
  **Then** validation rejects the value offline against the snapshot, no mutation is issued, and the command exits with an error naming the invalid value.

- **Given** an issue that is a lazyspec `story` (carrying the `lazyspec:story` label) and a GitHub `Bug`
  **When** `issue_type` is read or written
  **Then** the `lazyspec:{type}` label is never read, written, or otherwise affected — the two are fully orthogonal.

## Scope

### In Scope

- Populating the `issue_type` document attribute from the issue's native `issueType` on fetch (read path).
- Writing `issue_type` via `update --attr issue_type=<value>`, issuing the `updateIssue` mutation with `issueTypeId` resolved from the cached schema snapshot.
- Clearing `issue_type` (`--attr issue_type=`) by issuing `updateIssue` with `issueTypeId` null.
- Offline validation of the `issue_type` value against the org issue-type list in the schema snapshot.
- `issue_type` surfaced in `show --json` / `status --json` like any other attribute.

### Out of Scope

- Any change to the lazyspec type label (`lazyspec:{type}`); it remains the sole source of lazyspec document type and is untouched here.
- Project (Projects v2) field values and the `PROJECT-n.<field>` namespaced attributes.
- Sub-issues and subdirectory child materialization.
- Authoring or editing the org's issue-type definitions themselves (lazyspec only selects from existing types).
- Conflict detection on the write; last-write-wins per RFC-050.
