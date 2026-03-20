---
title: CLI spinners for git remote operations
type: story
status: draft
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/rfcs/RFC-029-async-git-operations-with-progress-feedback.md
---


## Context

RFC-029 introduced git remote operations into the document creation path via the reservation system (RFC-028). These operations run synchronously, leaving the CLI with no feedback during potentially slow network calls. Users cannot distinguish "working" from "stuck." This story adds `indicatif` spinners to the CLI so that every blocking git remote operation shows progress on stderr, while keeping stdout clean for `--json` output.

## Acceptance Criteria

- **Given** numbering is configured as `reserved` and the user runs `lazyspec create`
  **When** the CLI calls `reserve_next` to query and push to the remote
  **Then** a spinner appears on stderr showing "Querying remote for existing reservations..." followed by "Reserving RFC-NNN (attempt N/M)..." on each push attempt, and clears on success

- **Given** numbering is configured as `reserved` and the user runs `lazyspec create --json`
  **When** the reservation remote operation executes
  **Then** no spinner output appears on stderr, and stdout contains only the JSON result

- **Given** the user runs `lazyspec reservations list`
  **When** the CLI fetches refs from the remote
  **Then** a spinner appears on stderr showing "Querying remote..." and clears when results arrive before the table is printed

- **Given** the user runs `lazyspec reservations list --json`
  **When** the CLI fetches refs from the remote
  **Then** no spinner output appears on stderr

- **Given** the user runs `lazyspec reservations prune`
  **When** stale reservation refs are found and deleted
  **Then** a spinner shows during the initial query, then a progress bar on stderr shows "Pruning [N/M] refs/reservations/..." for each deletion, followed by a summary on completion

- **Given** the user runs `lazyspec reservations prune --json`
  **When** stale reservation refs are found and deleted
  **Then** no spinner or progress bar appears on stderr

- **Given** a git remote operation is in progress
  **When** the operation is running
  **Then** the git operation runs on a spawned `std::thread` while the main thread drives the spinner, so the terminal remains responsive

- **Given** a git remote operation fails (network error, auth failure)
  **When** the spawned thread returns an error
  **Then** the spinner stops and the error is printed to stderr

## Scope

### In Scope

- `indicatif::ProgressBar` spinners on stderr for `create` (reserved numbering), `reservations list`, and `reservations prune`
- Spinner suppression when `--json` is passed
- Spawning git operations on a background `std::thread` with the main thread polling completion and ticking the spinner
- Progress bar (not just spinner) for `reservations prune` deletions showing item counts
- Consuming the `ReservationProgress` / `PruneProgress` callback API delivered by Story 1

### Out of Scope

- The `ReservationProgress` / `PruneProgress` callback API itself (Story 1)
- TUI integration or background workers for `submit_create_form` (Story 3)
- Changes to the reservation protocol or ref format (RFC-028)
- Async runtime introduction; this uses `std::thread` only
