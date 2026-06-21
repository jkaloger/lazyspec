---
title: Reloadable session config and config-file watching
type: story
status: accepted
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

The TUI loads `Config` once at startup in `src/tui/infra/event_loop.rs::run` and threads it by `&Config`/clone through the run loop and `handle_app_event`, making it an immutable session constant. The `notify` file watcher is established once over `config.documents.types[].dir` and ignores `.lazyspec.toml` entirely. Consequently nothing can change configuration for a running session: a different type list, a renamed type directory, or an externally edited config file have no effect until restart. This slice converts the session `Config` into a reloadable value and introduces a single reload primitive that re-loads `Config` from `.lazyspec.toml`, rebuilds the `Store`, re-establishes the watcher over the (possibly changed) type directories, and redraws. It also adds `.lazyspec.toml` to the watch set so an external change reloads the session when there are no unsaved edits. This is the foundation that the save action (slice 3) will call; this slice ships the machinery and a manual/no-op reload trigger to validate it, without any settings-editing UI.

## Acceptance Criteria

### AC1: Session config is held as reloadable state, not a startup constant

**Given** the TUI is running with a `Config` loaded from `.lazyspec.toml`
**When** the reload primitive runs with a modified `.lazyspec.toml` on disk
**Then** subsequent draws, validation, and store operations use the newly loaded `Config` rather than the `Config` captured at startup.

### AC2: Reload re-loads Config from disk

**Given** the TUI is running and `.lazyspec.toml` has been changed on disk
**When** the reload primitive is triggered
**Then** `Config` is re-parsed from the current `.lazyspec.toml` and the new value becomes the active session config.

### AC3: Reload rebuilds the Store

**Given** a reload that changes the configured document types or a type's directory
**When** the reload primitive runs
**Then** the `Store` is rebuilt via `Store::load` against the freshly loaded `Config` so the document tree reflects the new type set and directories.

### AC4: Reload re-establishes the watcher over the new type directories

**Given** a reload in which a type's `dir` is added, removed, or renamed
**When** the reload primitive completes
**Then** the file watcher is watching exactly the existing type directories defined by the new `Config`, and is no longer watching directories that the new `Config` does not define.

### AC5: `.lazyspec.toml` is in the watch set

**Given** the TUI is running
**When** the watcher is established at startup and after any reload
**Then** `.lazyspec.toml` itself is watched in addition to the type directories.

### AC6: External change to `.lazyspec.toml` triggers reload when the buffer is clean

**Given** the TUI is running with no unsaved settings edits
**When** `.lazyspec.toml` is modified externally (for example by `git pull`)
**Then** the reload primitive runs, applying the new `Config`, rebuilt `Store`, and re-established watch set, and the screen redraws to reflect the change.

### AC7: Reload redraws against the new state

**Given** a completed reload that changed the document type list or tree
**When** the next frame is drawn
**Then** the UI renders against the rebuilt `Store` and new `Config` without requiring a restart.

### AC8: A reload failure leaves the running session intact

**Given** the TUI is running with a valid active `Config`
**When** the reload primitive is triggered but `.lazyspec.toml` fails to parse or `Store::load` returns an error
**Then** the previously active `Config`, `Store`, and watch set remain in effect and the session continues running.

## Scope

### In Scope
- Converting the session `Config` from a startup constant into reloadable state owned by the run loop / app, replacing the `&Config` threading where needed for live reassignment.
- A single reload primitive that, in order: re-loads `Config` from `.lazyspec.toml`, rebuilds the `Store` via `Store::load`, re-establishes the `notify` watcher over the current type directories, and requests a redraw.
- Adding `.lazyspec.toml` to the watch set at startup and after every reload.
- Detecting external modification of `.lazyspec.toml` and invoking the reload primitive when there are no unsaved settings edits.
- A manual/no-op trigger sufficient to exercise and validate the reload primitive end to end.
- Preserving the existing session when a reload fails (parse error or `Store::load` error).

### Out of Scope
- The settings view and any UI for displaying or editing configuration (owned by slice 1).
- The dirty edit buffer, the save action that calls this reload primitive, and the discard interaction (owned by slice 3).
- The dirty-buffer-versus-external-change conflict prompt (warn/keep/discard) when `.lazyspec.toml` changes externally while edits are unsaved (owned by slice 3); this slice handles only the clean-buffer reload-on-external-change case.
- Lease management for the config file; `.lazyspec.toml` is not a lease-managed document (leases remain scoped to documents).

