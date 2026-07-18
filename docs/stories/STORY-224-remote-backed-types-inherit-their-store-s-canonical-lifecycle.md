---
title: "Remote-backed types inherit their store's canonical lifecycle"
type: story
status: complete
author: "unknown"
date: 2026-07-19
tags: []
related:
- related-to: BUG-008
- related-to: STORY-223
---
<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

STORY-223 wired sync to map a remote issue/milestone `open`/`closed` onto the
type's **declared** lifecycle (`first_active_status()` / `terminal_status()`).
It assumed every remote-backed type declares a lifecycle. Types that declare
none (this repo's own `milestone` type) fall through to the hardcoded
`draft`/`complete` fallback in `Lifecycle::seed_status()` — so every open
milestone shows `draft` in the TUI, the status picker offers no transitions
(empty edges → `targets_from` returns nothing), and the DAG a user expects from
GitHub (`open`/`closed`) is invisible. This is the unfinished half of BUG-008:
"define a lifecycle mapping per github-backed type."

A remote store knows its own canonical state model — GitHub milestones and
issues are `open`/`closed`; a ClickUp list defines its own status set. The value
here is that a remote-backed type gets that canonical lifecycle **for free**,
without hand-declaring it in `.lazyspec.toml`, while a type that *does* declare a
lifecycle keeps overriding it.

## Acceptance Criteria

- **AC1 (walking skeleton — milestones):**
  **Given** a `github-milestones`-backed type with no `lifecycle` declared,
  **When** milestones sync,
  **Then** an `open` milestone shows status `open` and a `closed` one shows
  `closed`, and the TUI/CLI status DAG for that type is `open` ↔ `closed`
  (both directions), sourced from the store's canonical lifecycle.

- **AC2 (issues):**
  **Given** a `github-issues`-backed type with no `lifecycle` declared,
  **When** issues sync,
  **Then** status and DAG behave as AC1 (`open` ↔ `closed`).

- **AC3 (declared lifecycle wins):**
  **Given** a remote-backed type that **does** declare a non-empty `lifecycle`,
  **When** it syncs,
  **Then** its declared lifecycle is used unchanged (STORY-223 behaviour intact),
  and the store's canonical lifecycle is ignored.

- **AC4 (clickup — dynamic from remote):**
  **Given** a `clickup-tasks`-backed type with no `lifecycle` declared,
  **When** tasks sync,
  **Then** the type's effective lifecycle is the list's own status set as
  reported by the remote (not a fixed `open`/`closed`), and each task's status
  reflects its remote status.

- **AC5 (bidirectional transition still gated / write-through):**
  **Given** a synced milestone/issue at `closed`,
  **When** a user transitions it to `open` in the TUI/CLI,
  **Then** the transition is permitted (canonical edge exists) and follows the
  existing STORY-223 write-through model to reopen the remote counterpart.

- **AC6 (filesystem / git-ref unaffected):**
  **Given** a `filesystem` or `git-ref` type,
  **When** anything reads its lifecycle,
  **Then** behaviour is unchanged — no canonical lifecycle is injected; a
  declared-or-empty lifecycle resolves exactly as today.

## Scope

### In Scope

- `TypeDef::effective_lifecycle()` resolver: the declared lifecycle when it names
  any states, else the store backend's canonical lifecycle
  (`StoreBackend::canonical_lifecycle()`). Consulted by every reader of a type's
  lifecycle: status derivation (`milestone_state_to_status`, `issue_cache`
  first-active/terminal), birth seed, transition gate (`update`), TUI status DAG
  (`open_status_picker`, status filter union), status membership
  (`accepts_status`), and write-through open/closed classification.
- Static canonical lifecycle for `github-milestones` / `github-issues`
  (`states=[open,closed]`, empty edges → unconstrained → bidirectional DAG).
- No config mutation, no runtime derivation ordering bug: github status is
  lifecycle-derived at cache-write time, so a resolver (not persist-after-sync)
  is required for correctness on the first fetch.

### Out of Scope

- **AC4 (ClickUp) already delivered** (RFC-056: `derive_lifecycle` +
  `persist_clickup_lifecycles` writes the list's status set into config, so the
  declared lifecycle is non-empty and `effective_lifecycle` returns it). This
  story only regression-guards it.
- Persist-on-sync for github (rejected: ClickUp status is raw remote, github is
  lifecycle-derived — persisting after the cache write would show `draft` until a
  second fetch).
- Per-state custom mapping config (partial declared lifecycle merged with
  canonical) — canonical only fills the fully-empty case.
- `github-projects` canonical lifecycle (project column/status model) — deferred.
- Richer GitHub issue state (issue-type / project state) beyond `open`/`closed`.
