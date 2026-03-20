---
title: Reserved numbering config and validation
type: story
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/rfcs/RFC-028-git-based-document-number-reservation.md
---



## Context

RFC-028 introduces a `reserved` numbering strategy that coordinates document number allocation across distributed contributors via git refs. Before any git plumbing can work, the configuration layer needs to parse and validate the new `[numbering.reserved]` section, introduce the `ReservedConfig` and `ReservedFormat` types, and wire them into the existing `NumberingStrategy` enum.

This story covers the foundational config and type work. Without correct parsing and validation here, the reservation mechanism (Story 1) and management commands (Story 3) have nothing to build on.

## Acceptance Criteria

- **Given** a `lazyspec.toml` with `numbering = "reserved"` and a valid `[numbering.reserved]` section
  **When** the config is loaded
  **Then** a `NumberingStrategy::Reserved` variant is produced containing a `ReservedConfig` with the specified `remote`, `format`, and `max_retries`

- **Given** a `[numbering.reserved]` section that omits `remote` and `max_retries`
  **When** the config is loaded
  **Then** `remote` defaults to `"origin"` and `max_retries` defaults to `5`

- **Given** `format = "sqids"` in `[numbering.reserved]`
  **When** the config is loaded
  **Then** validation confirms that a `[numbering.sqids]` section with `salt` and `min_length` is also present, and fails with a clear error if it is missing

- **Given** `format = "incremental"` in `[numbering.reserved]`
  **When** the config is loaded
  **Then** no `[numbering.sqids]` section is required

- **Given** `remote = ""` (empty string) in `[numbering.reserved]`
  **When** the config is loaded
  **Then** validation fails with an error indicating the remote name must be non-empty

- **Given** `format = "sqids"` with a valid `[numbering.sqids]` section
  **When** a document number is formatted
  **Then** the raw integer is encoded through sqids to produce the filename segment

- **Given** `format = "incremental"`
  **When** a document number is formatted
  **Then** the raw integer is used directly (zero-padded) as the filename segment

## Scope

### In Scope

- `[numbering.reserved]` config section parsing (remote, format, max_retries)
- `ReservedConfig` and `ReservedFormat` types
- New `Reserved` variant on the `NumberingStrategy` enum
- `format` field dispatch to incremental or sqids encoding for filenames
- Validation: non-empty remote name
- Validation: `[numbering.sqids]` required when `format = "sqids"`
- Default values: `remote = "origin"`, `max_retries = 5`

### Out of Scope

- Git plumbing operations (ref creation, atomic push, fetch)
- Retry loop logic for push conflicts
- `create` command integration with the reservation flow
- `reservations list` / `reservations prune` subcommands
