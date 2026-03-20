---
title: TUI async document creation
type: story
status: draft
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/rfcs/RFC-029-async-git-operations-with-progress-feedback.md
---


## Context

The TUI's `submit_create_form` calls `create::run` on the main thread. When the document type uses reserved numbering, this triggers synchronous git remote operations (`ls-remote`, `push`) that block the event loop. During this time the terminal is unresponsive: no rendering, no input handling, no file watch events. A slow remote can freeze the UI for several seconds with no indication of what's happening.

The TUI already has a pattern for background work (the expansion worker uses `std::thread` + `crossbeam_channel` + `AppEvent`). This story applies the same pattern to document creation when reserved numbering is active, keeping the main thread free and giving the user visual feedback while the reservation completes.

## Acceptance Criteria

- **Given** the user fills out the create form for a document type with `numbering = "reserved"`
  **When** they submit the form
  **Then** a background thread is spawned to run `create::run`, and the main event loop continues processing input and rendering without blocking

- **Given** the create form has been submitted for a reserved-numbering document type
  **When** the background thread begins execution
  **Then** an `AppEvent::CreateStarted` event is emitted and the form transitions to a loading state where inputs are disabled and a "Reserving..." status message is shown

- **Given** the background thread is running `create::run` with a progress callback
  **When** the reservation module reports progress (e.g. querying remote, push attempt)
  **Then** `AppEvent::CreateProgress` events are sent through the channel and the create form's status message updates to reflect the current step

- **Given** the background thread completes `create::run` successfully
  **When** `AppEvent::CreateComplete` arrives with an `Ok(CreateResult)`
  **Then** the create form closes and the TUI navigates to the newly created document

- **Given** the background thread completes `create::run` with an error
  **When** `AppEvent::CreateComplete` arrives with an `Err`
  **Then** the create form re-enables its inputs and displays the error message in place, allowing the user to retry or cancel

- **Given** the user fills out the create form for a document type that does not use reserved numbering (incremental, sqids)
  **When** they submit the form
  **Then** `create::run` executes synchronously on the main thread as it does today, with no background thread spawned

- **Given** the create form is in the loading state (background reservation in progress)
  **When** the user presses Escape or the cancel key
  **Then** the form closes but the background thread is allowed to finish (no thread cancellation); the result is silently discarded when `AppEvent::CreateComplete` arrives for a form that is no longer open

## Scope

### In Scope

- `AppEvent::CreateStarted`, `AppEvent::CreateProgress`, and `AppEvent::CreateComplete` event variants
- `CreateResult` struct containing the created document's path and type
- Background thread spawn in `submit_create_form` when the document type uses reserved numbering
- Progress callback wiring: the background thread forwards `ReservationProgress` values as `AppEvent::CreateProgress` messages
- Loading state in the create form: disabled inputs, status message area
- Error display in the create form on failure
- Navigation to the new document on success
- Non-reserved document types remain synchronous

### Out of Scope

- The `ReservationProgress` / `PruneProgress` callback API itself (Story 1 delivers this)
- CLI changes, spinners, or `indicatif` integration (Story 2)
- Changes to the reservation protocol or git plumbing (RFC-028)
- Thread cancellation or abort mechanisms for in-flight reservations
- Retry UI beyond re-enabling the form inputs after an error
