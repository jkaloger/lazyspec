---
title: Core reservation mechanism
type: story
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/rfcs/RFC-028-git-based-document-number-reservation.md
---



## Context

Document numbering in lazyspec is local-only. Both the incremental and sqids strategies scan the local filesystem to pick the next ID, which means two people branching from the same state will silently produce colliding document numbers. The collision is only discovered at merge time.

This story introduces a `NumberingStrategy::Reserved` variant and the underlying git plumbing needed to reserve document numbers on the remote before creating files locally. By using git custom refs (`refs/reservations/{PREFIX}/{NUM}`) with atomic push, we guarantee that no two branches can claim the same number. The reservation happens in the command layer so that `resolve_filename` stays pure -- it receives a pre-computed number rather than needing git access.

## Acceptance Criteria

- **Given** a project configured with `numbering = "reserved"` and a reachable remote
  **When** `lazyspec create` runs
  **Then** it queries `refs/reservations/{PREFIX}/*` on the remote, picks the next number, creates a local ref, and atomically pushes it before writing the document file

- **Given** a concurrent reservation attempt where the atomic push fails (ref already exists on remote)
  **When** the push is rejected
  **Then** the local ref is cleaned up, the number is incremented, and the push is retried up to the configured maximum (default 5 attempts)

- **Given** the retry loop has exhausted all attempts
  **When** every push is rejected
  **Then** `create` fails with a clear error message and does not write a document file

- **Given** the remote is unreachable (offline, no remote configured, auth failure)
  **When** `lazyspec create` runs with `numbering = "reserved"`
  **Then** it fails immediately with an error explaining that reserved numbering requires remote access, and suggests `--numbering incremental` or `--numbering sqids` as a one-off override

- **Given** the reservation succeeds
  **When** the reserved number is passed to `resolve_filename`
  **Then** the number is used as-is and the template layer does not perform any git operations

## Scope

### In Scope

- `NumberingStrategy::Reserved` enum variant
- Git plumbing integration: `ls-remote`, `hash-object`, `update-ref`, `push`
- Atomic push with bounded retry loop
- Integration with the `create` command to reserve before resolving the filename
- Graceful failure with actionable error message when the remote is unreachable

### Out of Scope

- Config parsing for `[numbering.reserved]` (Story 2)
- Format dispatch between incremental and sqids encoding (Story 2)
- `ReservedConfig` / `ReservedFormat` validation rules (Story 2)
- `lazyspec reservations list` and `reservations prune` subcommands (Story 3)
- Orphan detection and `--dry-run` support (Story 3)
