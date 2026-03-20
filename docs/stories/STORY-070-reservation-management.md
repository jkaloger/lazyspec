---
title: Reservation management
type: story
status: draft
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/rfcs/RFC-028-git-based-document-number-reservation.md
---


## Context

As projects grow, reservation refs accumulate in the `refs/reservations/*` namespace. Teams need visibility into what numbers have been reserved and by whom, and a way to clean up refs for documents that have already been created. Without management commands, the reservation namespace becomes cluttered and opaque.

This story covers the `lazyspec reservations list` and `lazyspec reservations prune` subcommands, giving users the ability to query, inspect, and clean up reservation refs from the remote.

## Acceptance Criteria

- **Given** reservation refs exist on the remote
  **When** the user runs `lazyspec reservations list`
  **Then** all reservation refs are displayed, showing the document type, number, and ref path

- **Given** reservation refs exist on the remote
  **When** the user runs `lazyspec reservations list --json`
  **Then** the output is structured JSON containing each reservation's type, number, and ref

- **Given** a reservation ref exists for a document that has been created locally (e.g. `refs/reservations/RFC/042` and `RFC-042-*.md` exists)
  **When** the user runs `lazyspec reservations prune`
  **Then** the matching reservation ref is deleted from the remote

- **Given** a reservation ref exists for a document that has not been created locally
  **When** the user runs `lazyspec reservations prune`
  **Then** the ref is flagged as an orphan in the output but is not deleted

- **Given** reservation refs exist, some with matching documents and some without
  **When** the user runs `lazyspec reservations prune --dry-run`
  **Then** the output shows which refs would be pruned and which are orphaned, without deleting anything

- **Given** reservation refs exist
  **When** the user runs `lazyspec reservations prune --json`
  **Then** the output is structured JSON listing pruned refs, orphaned refs, and any errors

## Scope

### In Scope

- `lazyspec reservations list` subcommand that queries the remote via `git ls-remote`
- `lazyspec reservations prune` subcommand that deletes refs whose documents exist locally
- `--dry-run` flag for prune that previews actions without modifying the remote
- Orphan detection: reservations with no matching local document are flagged but not deleted
- `--json` output support for both `list` and `prune`

### Out of Scope

- Core reservation mechanism (creating refs during `create`) -- covered by Story 1
- Config parsing for remote name and numbering settings -- covered by Story 2
- Format dispatch and validation rules -- covered by Story 2
