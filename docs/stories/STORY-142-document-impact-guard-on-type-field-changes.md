---
title: Document-impact guard on type field changes
type: story
status: accepted
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

The TUI settings screen (RFC-023) edits `.lazyspec.toml` as a dirty buffer committed atomically with `w` (slice 3). Three fields on a `[[types]]` entry are load-bearing for documents already on disk: `dir` (where a type's documents live), `prefix` (the ID prefix new documents inherit), and `store` (the backend the type points at). Changing any of these on a type that already has documents silently orphans or desyncs those documents — the existing files are not moved, renamed, or renumbered by the settings screen, which edits config only. This slice inserts a detection-and-confirmation step into slice 3's save flow: before an atomic save that would alter a load-bearing field of a type with existing documents, the screen pauses, reports the affected documents and the consequence, and requires explicit confirmation. The settings screen never migrates documents; that is the user's responsibility and is out of scope for RFC-023 entirely.

## Acceptance Criteria

### AC1: Guard triggers on load-bearing change with existing documents

**Given** the dirty buffer changes a type's `dir`, `prefix`, or `store`, and that type has one or more existing documents enumerable via `Store::list` filtered by the type
**When** the user issues the save command (`w`)
**Then** the save pauses before writing and a confirmation guard is shown instead of committing the buffer.

### AC2: Guard reports affected documents and consequence

**Given** the guard has been triggered for a load-bearing field change
**When** the guard is displayed
**Then** it states the number of affected documents (or lists them), names the field being changed with its old and new values, and explains the consequence in plain language (for example, "12 documents in `docs/rfcs` will no longer be found; files are not moved").

### AC3: Confirming writes the changed config

**Given** the guard is displayed for a triggered load-bearing change
**When** the user explicitly confirms
**Then** the dirty buffer is committed atomically to `.lazyspec.toml` exactly as slice 3 would write it, and no document files are moved, renamed, or renumbered.

### AC4: Cancelling preserves the buffer and config

**Given** the guard is displayed for a triggered load-bearing change
**When** the user declines confirmation
**Then** no write occurs, `.lazyspec.toml` on disk is unchanged, and the dirty buffer retains the pending edit so the user can amend or discard it.

### AC5: Non-load-bearing changes save without the guard

**Given** the dirty buffer changes only fields that are not load-bearing for existing documents (for example `icon` or `plural`), or changes a load-bearing field on a type that has zero existing documents
**When** the user issues the save command
**Then** the buffer is committed atomically with no guard shown, identical to slice 3's behaviour.

### AC6: Guard scopes affected counts per changed type

**Given** the dirty buffer changes load-bearing fields on more than one type, each with existing documents
**When** the guard is displayed
**Then** the affected-document count and consequence are reported per changed type, attributing each count to the specific type whose field changed.

## Scope

### In Scope
- Detecting, at save time, whether the dirty buffer alters a `[[types]]` entry's `dir`, `prefix`, or `store` relative to the on-disk config.
- Determining whether each such changed type has existing documents on disk via the store's type-filtered enumeration.
- Presenting a confirmation guard that reports the affected document count (or list), the field and its old/new values, and a plain-language consequence statement.
- Requiring explicit confirmation before the atomic write proceeds, and preserving the dirty buffer on cancellation.
- Passing through to the existing save behaviour unchanged when no load-bearing field changed or the affected type has zero documents.

### Out of Scope
- The scalar editing and atomic save machinery itself, including the dirty buffer and `w` write path — owned by slice 3; this slice only inserts a confirmation step into that flow.
- Migrating, moving, renaming, or renumbering documents to match the new field values — explicitly not part of RFC-023 at all; the user performs any file migration themselves.
- The read-only settings view — owned by slice 1.
- Reloading config after external changes — owned by slice 2.
- Adding or deleting collection (`[[types]]`) entries — owned by slice 5.

