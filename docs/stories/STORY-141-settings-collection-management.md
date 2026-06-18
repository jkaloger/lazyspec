---
title: Settings collection management
type: story
status: draft
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

This slice adds entry-level mutation to the collection sections of the settings screen — the `[[types]]`, `[[relationships]]`, and `[[rules]]` arrays plus the keyed `[certification.overrides]` map. Slice 1 already renders each collection as a drill-in entry list and slice 3 already edits the scalar fields within an entry; this slice layers the create and delete operations on top. Pressing `n` while focused on a collection seeds a new entry from a sensible default shape (mirroring `starter_types()` / `default_rules()`) and immediately drills into it so the user fills its fields; pressing `d` deletes the selected entry behind a confirmation that reuses the existing `DeleteConfirm` pattern. One guard is enforced: the last `[[relationships]]` entry cannot be deleted, because a config with no relationships is a hard load error under ADR-011. All mutations operate on the in-memory dirty buffer and are persisted only by slice 3's atomic save.

## Acceptance Criteria

### AC1: Seeding a new types entry drills in with default fields

**Given** the user is focused on the `[[types]]` collection list
**When** the user presses `n`
**Then** a new entry seeded from the `starter_types()` `TypeDef` shape (e.g. incremental numbering, filesystem store, empty `agents`) is appended to the dirty buffer and the screen drills into that entry's field list with a breadcrumb so the user can edit its fields.

### AC2: Seeding a new rule entry uses the parent-child default shape

**Given** the user is focused on the `[[rules]]` collection list
**When** the user presses `n`
**Then** a new `ValidationRule::ParentChild` entry (the `default_rules()` parent-child shape) is appended to the dirty buffer and the screen drills into it.

### AC3: Seeding an overrides entry prompts for a spec-path key

**Given** the user is focused on the `[certification.overrides]` map
**When** the user presses `n`
**Then** the user is prompted for a spec-path key, and on confirmation a new override entry keyed by that path (with default `normalize`) is inserted into the dirty buffer and the screen drills into it.

### AC4: Deleting an entry requires confirmation

**Given** an entry in any collection is selected
**When** the user presses `d`
**Then** a delete confirmation (following the `DeleteConfirm` pattern) is shown naming the entry, and the entry is removed from the dirty buffer only after the user confirms.

### AC5: Cancelling a delete leaves the entry intact

**Given** the delete confirmation for a selected entry is shown
**When** the user cancels the confirmation
**Then** the entry remains in the dirty buffer unchanged and the screen returns to the collection list.

### AC6: Deleting the last relationship entry is refused

**Given** exactly one entry remains in the `[[relationships]]` collection and it is selected
**When** the user presses `d`
**Then** the deletion is refused (no confirmation is shown and the entry is not removed), because a config with no `[[relationships]]` is a hard load error under ADR-011.

### AC7: Deleting a non-last relationship entry is allowed

**Given** two or more entries exist in the `[[relationships]]` collection and a non-last one is selected
**When** the user presses `d` and confirms
**Then** the selected entry is removed from the dirty buffer and at least one relationship entry remains.

### AC8: Created and deleted entries persist through save

**Given** the user has seeded a new entry and deleted another in a collection
**When** the dirty buffer is saved
**Then** the persisted config reflects exactly the added and removed entries, and the resulting config is one that strict load accepts.

## Scope

### In Scope
- `n` to seed a default entry into `[[types]]`, `[[relationships]]`, `[[rules]]`, and `[certification.overrides]`, then drill into it.
- Default seed shapes drawn from `starter_types()` (types), `default_rules()` parent-child (rules), a single `RelationshipDef`, and a default-`normalize` `CertificationOverride` for the map.
- Spec-path key prompt when seeding a `[certification.overrides]` entry.
- `d` to delete the selected entry behind a `DeleteConfirm`-style confirmation, with cancel leaving the buffer unchanged.
- The ADR-011 guard refusing deletion of the last `[[relationships]]` entry.
- Producing buffer mutations that slice 3's atomic save persists.

### Out of Scope
- Editing the scalar fields within an entry — owned by slice 3 (scalar field editors).
- Read-only drill-in navigation and breadcrumb rendering — owned by slice 1.
- Dependency auto-scaffolding when an entry is added — owned by slice 4.
- The atomic save and load-validation machinery that persists and validates the mutated buffer — owned by slice 3.
- The doc-impact guard — owned by slice 6.

