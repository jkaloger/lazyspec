---
title: Retire STORY-015 graph mode
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
---


## Summary

Retire the legacy STORY-015 graph mode once the new sequencing screen lands. The old read-only `implements`-tree graph view is superseded by the interactive sequencing screen described in RFC-041. This slice removes the legacy entry point, migrates or drops its keybindings, refreshes `--help` and in-app help, and audits docs and skills so no references to the retired mode remain.

## Scope

### In Scope

- Removal of the legacy graph rendering code path and its associated routes/entry points.
- Removal or remapping of legacy keybindings to the new sequencing screen.
- Updates to `--help` (CLI) and any in-app help screens.
- Audit and migration of references to the old graph mode across docs and skills.
- Marking STORY-015 as superseded if appropriate.

### Out of Scope

- The new sequencing screen itself (Stories 4a/4b/4c) and any of its render/edit/overlay behaviour.
- Engine graph primitives (Story 1).
- Priority field and TOML config (Story 2).
- CLI commands `next`, `graph`, `critical-path` (Story 3).
- Skills `/sequence` and `/next-work` (Stories 5 and 6).

## Acceptance Criteria

- **Given** the new sequencing screen has landed
  **When** a user attempts to launch the legacy STORY-015 graph mode via its previous entry point
  **Then** the legacy entry point is no longer present and the user is directed to the new sequencing screen.

- **Given** a user inside the TUI
  **When** they press a key that previously triggered the legacy graph mode
  **Then** the key is either unbound or maps to the new sequencing screen, and no legacy graph rendering is shown.

- **Given** a user runs `lazyspec --help` or any subcommand `--help`
  **When** they read the output
  **Then** no legacy graph mode is referenced and any sequencing-related entry points are described accurately.

- **Given** a user opens any in-app help screen
  **When** they read its contents
  **Then** there are no references to the retired graph mode and any listed graph entry points point to the new sequencing screen.

- **Given** a reader browsing project docs and skills
  **When** they search for references to the legacy STORY-015 graph mode
  **Then** no references remain, except where preserved as historical context with an explicit superseded marker.

- **Given** STORY-015 is no longer the active graph implementation
  **When** its status is reviewed
  **Then** it is marked superseded (or otherwise terminal) so it does not appear as ready work.

- **Given** the legacy graph mode is retired
  **When** `lazyspec validate --json` runs over the repo
  **Then** validation passes with no errors introduced by the retirement.
