---
title: Progress-aware reservation API
type: story
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: RFC-029
---




## Context

The reservation system (RFC-028) added synchronous git remote operations to the document creation path. Every `reserve_next`, `list_reservations`, and `delete_remote_ref` call blocks with no way for the caller to know what stage the operation has reached. RFC-029 addresses this by introducing a callback-based progress reporting mechanism in the engine layer, keeping the API synchronous but giving callers the information they need to build progress UI.

This story adds the `ReservationProgress` and `PruneProgress` enums, threads a callback parameter through the three public reservation functions, and adds `indicatif` as a dependency for downstream stories. All existing callers are updated with no-op callbacks so behaviour is unchanged.

## Acceptance Criteria

- **Given** the `reservation` module defines `reserve_next`
  **When** the function signature is updated to accept an `on_progress: impl Fn(ReservationProgress)` callback
  **Then** the callback is invoked with `QueryingRemote` before `ls_remote`, with `PushAttempt` before each push attempt, with `PushRejected` when a push is rejected, and with `Reserved` when a number is successfully reserved

- **Given** the `reservation` module defines `list_reservations`
  **When** the function signature is updated to accept an `on_progress` callback
  **Then** the callback is invoked with a progress variant before running `git ls-remote`

- **Given** the `run_prune` function calls `list_reservations` and `delete_remote_ref` in a loop
  **When** the prune workflow is updated to accept an `on_progress: impl Fn(PruneProgress)` callback
  **Then** the callback is invoked with `QueryingRemote` before listing, with `Deleting { current, total, ref_path }` before each deletion, and with `Done { pruned, orphaned }` when pruning completes

- **Given** a `ReservationProgress` enum exists with variants `QueryingRemote`, `PushAttempt { attempt, max, candidate }`, `PushRejected { candidate }`, and `Reserved { number }`
  **When** the enum is compiled
  **Then** it derives `Debug` and `Clone`

- **Given** a `PruneProgress` enum exists with variants `QueryingRemote`, `Deleting { current, total, ref_path }`, and `Done { pruned, orphaned }`
  **When** the enum is compiled
  **Then** it derives `Debug` and `Clone`

- **Given** the CLI `create` command calls `reserve_next`
  **When** the callback parameter is added
  **Then** the CLI caller passes a no-op closure `|_| {}` so existing behaviour is unchanged

- **Given** the CLI `reservations list` command calls `list_reservations`
  **When** the callback parameter is added
  **Then** the CLI caller passes a no-op closure so existing behaviour is unchanged

- **Given** the CLI `reservations prune` command calls the prune workflow
  **When** the callback parameter is added
  **Then** the CLI caller passes a no-op closure so existing behaviour is unchanged

- **Given** integration tests in `reservation_test.rs` call `reserve_next` and `list_reservations`
  **When** the callback parameter is added
  **Then** all tests pass with no-op callbacks and no other changes

- **Given** `Cargo.toml` lists the project dependencies
  **When** `indicatif` is added as a dependency
  **Then** `cargo build` succeeds and `indicatif` is available for use in downstream stories

## Scope

### In Scope

- `ReservationProgress` enum definition in `src/engine/reservation.rs`
- `PruneProgress` enum definition in `src/engine/reservation.rs` (or a sibling module)
- `on_progress` callback parameter added to `reserve_next` and `list_reservations`
- `on_progress` callback parameter threaded through the prune workflow (`run_prune` or equivalent)
- Progress callback invocations at each relevant step within the engine functions
- No-op callback updates to all existing callers: `src/cli/create.rs`, `src/cli/reservations.rs`, and `tests/reservation_test.rs`
- `indicatif` added to `[dependencies]` in `Cargo.toml`

### Out of Scope

- CLI spinner/progress bar UI (Story 2)
- TUI async integration or `AppEvent` variants (Story 3)
- Changes to the reservation protocol itself (ref format, retry logic, etc.)
- Any async runtime or threading changes; this story keeps all functions synchronous
