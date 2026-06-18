---
title: fix --config migration
type: story
status: accepted
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-042
---

## Context

Strict load (RFC-042) makes a missing `[[relationships]]` or `[[rules]]` block a hard error, which breaks every existing project on upgrade since the four historical relationships and three historical rules were previously implicit. `init` only scaffolds fresh projects and refuses when `.lazyspec.toml` exists, so it cannot repair an upgraded project. Per ADR-012, `fix` is the migration path: it already tolerates broken document frontmatter that normal load rejects, so it is extended one layer up to repair config. Because strict load would reject the very config `fix` must read, this requires a lenient config read that bypasses strict load — the chicken-and-egg that the lenient path resolves.

## Acceptance Criteria

- **Given** an existing `.lazyspec.toml` with `[[types]]` but no `[[relationships]]` or `[[rules]]`
  **When** the user runs `lazyspec fix --config`
  **Then** the four standard relationships (implements/implemented-by, supersedes/superseded-by, blocks/blocked-by, related-to symmetric) and the three standard rules (stories-need-rfcs warning, iterations-need-stories error, adrs-need-relations error) are injected into the file.

- **Given** the same config missing the standard blocks
  **When** the user runs `lazyspec fix --config --dry-run`
  **Then** the additions that would be made are reported and `.lazyspec.toml` is left unchanged on disk.

- **Given** a config that has been repaired by `fix --config`
  **When** any command loads the project under strict load
  **Then** the project loads with no strict-load error and existing relationship references validate against the injected set.

- **Given** documents with incomplete frontmatter alongside the config
  **When** the user runs `lazyspec fix --config`
  **Then** only `.lazyspec.toml` is modified and no document files are touched (config-only scope).

- **Given** a config still missing `[[relationships]]`
  **When** any command hits strict load and fails
  **Then** the emitted error names `lazyspec fix` as the remedy.

- **Given** a config already containing the standard blocks
  **When** the user runs `lazyspec fix --config` a second time
  **Then** no blocks are duplicated and the file is left unchanged (idempotent).

## Scope

### In Scope

- A `--config` flag on `fix` that scopes the run to config-only and skips all document fixes.
- A lenient config read path that bypasses strict load so `fix` can operate on a `.lazyspec.toml` missing `[[relationships]]`/`[[rules]]`.
- Injecting the missing standard relationship and rule blocks into an existing `.lazyspec.toml`.
- `--dry-run` previewing the additions without writing the file.
- Idempotent behaviour: running `fix --config` on an already-migrated config makes no changes.
- The strict-load error message naming `lazyspec fix` as the remedy.

### Out of Scope

- The fresh-project `init` scaffold writing the blocks for new projects (STORY-127).
- The relationship model and registry (STORY-126).
- Removing the doc-type defaults from the engine (STORY-125).
