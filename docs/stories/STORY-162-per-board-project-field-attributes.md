---
title: Per-board project field attributes
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

Project membership (STORY-161) puts a doc on a board but carries no field data. This slice surfaces a board's per-item field values. GitHub Projects v2 fields are typed (single-select, iteration, number, date, text) and their option sets live on the board, not in lazyspec config — so these are dynamic attributes validated against the cached schema snapshot (STORY-155), not declared `AttrDef`s.

Because a doc can be a member of several boards (STORY-161, many-to-many), the same field name can exist on two boards with different options. Field-value attributes are therefore namespaced by board id: `PROJECT-1.Status`, `PROJECT-2.Status` never collide. This is the hardest slice in RFC-050 — the write is a three-step id resolution with a strict value shape — so it is isolated from membership. It depends on STORY-161 (membership + board store), STORY-156 (attribute write path + round-trip), and STORY-155 (GraphQL + snapshot). Write policy is last-write-wins + refresh.

## Acceptance Criteria

- **Given** an issue-doc that is a member of a board with field values set
  **When** the doc is loaded
  **Then** each field surfaces as a `PROJECT-n.<field>` attribute, with GitHub field types mapped (single-select→enum, iteration→enum/string, number→int/float, date→date, text→string).

- **Given** the same field name (`Status`) on two boards the doc belongs to
  **When** both are read
  **Then** they appear as distinct attributes `PROJECT-1.Status` and `PROJECT-2.Status` with no collision.

- **Given** a single-select field
  **When** `PROJECT-n.<field>` is set via `--attr`
  **Then** the store resolves the project node id (reusing the id cached by STORY-161's membership binding, or a fresh lookup if absent), then the field id and option id from the snapshot, then calls `updateProjectV2ItemFieldValue` with a `value` object carrying exactly one key (`singleSelectOptionId`).

- **Given** an iteration field
  **When** `PROJECT-n.<field>` is set to an iteration
  **Then** the iteration id is resolved from the snapshot and the `value` object carries exactly one key, `iterationId`.

- **Given** a set single-select field
  **When** the attribute is cleared
  **Then** the store calls `clearProjectV2ItemFieldValue` (not an empty-string write).

- **Given** an option not present in the cached snapshot
  **When** the value is validated
  **Then** it is rejected offline against the snapshot before any mutation is attempted.

- **Given** number, date, and text fields
  **When** each is written
  **Then** the `value` object uses the correct single key (`number`, `date`, `text`) for that field type.

## Scope

### In Scope

- Reading per-item field values into namespaced `PROJECT-n.<field>` dynamic attributes, with type mapping.
- Board-id namespacing so same-named fields across boards do not collide.
- Writing single-select/iteration/number/date/text via `updateProjectV2ItemFieldValue` (single-key value) and clearing via `clearProjectV2ItemFieldValue`, using ids resolved from the snapshot.
- Offline validation of field values against the cached snapshot.

### Out of Scope

- Board membership itself (STORY-161) and the board store backend.
- Board `Status` driving lifecycle status — deferred in RFC-050; here `Status` is just one namespaced attribute among others.
- Comments (STORY-160).
- Conflict detection — last-write-wins + refresh per RFC-050. (Offline snapshot validation is best-effort: a truly stale/removed option still fails at the GraphQL mutation, which is the backstop per RFC-050 Risks.)
