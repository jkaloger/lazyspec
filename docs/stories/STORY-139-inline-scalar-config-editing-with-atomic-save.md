---
title: Inline scalar config editing with atomic save
type: story
status: accepted
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

The settings screen renders configuration fields in the right panel (slice 1) and can rebuild the store from a reloaded config (slice 2), but every field is read-only. This slice adds inline editing of existing scalar fields directly in the right panel — a new TUI pattern distinct from the modal overlays used for document creation and linking. Each scalar field type gets an appropriate editor: free text for `String`, a toggle for `bool`, a bounded numeric entry for `u8`/`usize`, a nullable text entry for `Option<String>`, a duration-string entry, a comma-separated entry for `Vec<String>`, and a variant cycler for enums. Confirmed edits accumulate in an in-memory `Config` buffer marked dirty; saving with `w` or `Ctrl-S` validates the entire buffer against the same field-level and cross-field constraints `Config::parse` enforces, then writes `.lazyspec.toml` exactly once via `toml_edit` so formatting and comments survive, and triggers the live reload. Editing never leaves an invalid intermediate state on disk.

## Acceptance Criteria

### AC1: Text field accepts free string input

**Given** the settings screen is focused on a `String` field (for example `naming.pattern`) in read-only state
**When** the user presses `Enter`, types a new value, and presses `Enter` again
**Then** the field shows the typed value, the change is recorded in the in-memory buffer, and a dirty indicator appears.

### AC2: Bool field toggles with Space

**Given** the settings screen is focused on a `bool` field (for example `tui.statusbar.enabled` or `subdirectory`) showing its current value
**When** the user presses `Space`
**Then** the displayed value flips between true and false, the new value is written into the buffer, and the dirty indicator appears.

### AC3: Bounded numeric editor rejects out-of-range input as typed

**Given** the settings screen is focused on a bounded numeric field (for example `sqids.min_length`, valid range 1..=10)
**When** the user enters edit mode and attempts to confirm a value outside the bound (such as `0` or `11`) or a non-numeric value
**Then** the editor rejects the entry, the buffer retains the prior valid value, and no dirty change is recorded for an invalid attempt.

### AC4: Nullable field distinguishes empty from unset

**Given** the settings screen is focused on an `Option<String>` field (for example `github.repo`)
**When** the user edits the field and confirms an empty value
**Then** the field is recorded in the buffer as unset (`None`) rather than an empty string, and confirming a non-empty value records `Some(value)`.

### AC5: Duration field rejects unparseable input

**Given** the settings screen is focused on a duration-string field (for example `coordination.lease_duration`, currently `60m`)
**When** the user confirms an unparseable duration (such as `abc` or `30`)
**Then** the editor rejects the entry and the buffer keeps the prior duration value.

### AC6: List field round-trips comma-separated values

**Given** the settings screen is focused on a `Vec<String>` field (for example a type's `agents` list)
**When** the user edits the field to a comma-separated string and confirms
**Then** the buffer records the trimmed list in entered order, and an empty entry records an empty list.

### AC7: Enum field cycles its declared variants with Space

**Given** the settings screen is focused on an enum field (for example `numbering` over incremental/sqids/reserved, `store` over filesystem/github-issues/git-ref, rule `severity` over error/warning, or rule `shape` over parent-child/relation-existence)
**When** the user presses `Space` repeatedly
**Then** the displayed value advances through that field's variant set in order and wraps to the first variant, recording each change in the buffer and setting the dirty indicator.

### AC8: Save validates the whole buffer and writes atomically

**Given** the buffer holds confirmed scalar edits and the dirty indicator is set
**When** the user presses `w` or `Ctrl-S` and the whole buffer satisfies field-level and cross-field constraints (mirroring `Config::parse`)
**Then** `.lazyspec.toml` is written exactly once via `toml_edit` with existing formatting and comments preserved, the live reload from slice 2 is triggered, and the dirty indicator clears.

### AC9: Failed save shows footer error and jumps to the offending field

**Given** the buffer holds edits that violate a cross-field constraint (for example a type set to `numbering = sqids` while `numbering.sqids.salt` is empty, or a type set to `store = github-issues` with no `[github]` section)
**When** the user presses `w` or `Ctrl-S`
**Then** no write occurs, a footer error describing the violated constraint is shown, focus jumps to the offending field, and the buffer stays dirty.

### AC10: Quitting with unsaved changes prompts to save or discard

**Given** the dirty indicator is set
**When** the user presses `q` or `Esc`
**Then** a save/discard prompt appears rather than an immediate exit, choosing save runs the same validate-and-write path as `w`, and choosing discard drops the buffer edits and leaves `.lazyspec.toml` untouched.

## Scope

### In Scope
- Inline editor for each scalar field type in the right panel: text (`String`), bool toggle via `Space`, bounded numeric (`u8`/`usize` with range enforcement), nullable (`Option<String>`, empty = unset), duration string, comma-separated list (`Vec<String>`), and enum cycle via `Space`.
- Enum variant sets: `numbering` (incremental/sqids/reserved), `store` (filesystem/github-issues/git-ref), reserved `format` (incremental/sqids), rule `severity` (error/warning), rule `shape` (parent-child/relation-existence).
- The in-memory dirty `Config` buffer accumulating confirmed edits, with a dirty indicator.
- Field-level validation as typed (out-of-range numeric, unparseable duration) rejecting malformed input before it enters the buffer.
- Whole-buffer validation on save (`w`/`Ctrl-S`) mirroring `Config::parse` field-level and cross-field constraints, a single atomic write of `.lazyspec.toml` via `toml_edit` preserving formatting and comments, and a failed-save footer error with focus jump to the offending field.
- The save/discard prompt on `q`/`Esc` when the buffer is dirty.
- Editing the scalar fields of existing entries (for example an existing type's `numbering` or `store`).

### Out of Scope
- The read-only field view and j/k navigation that this editing builds on — owned by slice 1 (assumed to exist).
- The reload-config-rebuild-store machinery invoked after a successful save — owned by slice 2 (this slice calls it).
- Dependency auto-scaffolding when an enum edit creates a section dependency (for example switching a type to `numbering = sqids` scaffolding a `[numbering.sqids]` section) — owned by slice 4.
- Collection entry add/delete via `n`/`d` — owned by slice 5; this slice edits existing scalar fields and existing entries' fields, not entry creation or removal.
- The document-impact confirmation for `dir`/`prefix`/`store` changes — owned by slice 6.
- Statusbar component ordering — owned by slice 7.

