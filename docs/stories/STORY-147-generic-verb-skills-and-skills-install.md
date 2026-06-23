---
title: Generic verb skills and skills install
type: story
status: accepted
author: jkaloger
date: 2026-06-21
tags: []
related:
- implements: RFC-048
---

## Context

Today's skills hard-code the `RFC → Story → Iteration` chain: `write-rfc`, `create-story`, `create-iteration`, `build`, `review-iteration`, `plan-work`. A user with a different DAG gets nothing that fits, and the skills push from planning into building rather than letting a planning phase settle. RFC-048's spine is **binary owns data, skill owns prose**: the binary serves config and state as JSON, the skill computes what to do from that data.

This Story ships the prose half. It replaces the hard-coded skills with one static, portable, DAG-agnostic set of generic verbs that take the type as a parameter and read `config --json` at runtime, so the same prose serves any DAG. It honors the authorship ceiling, never auto-crosses a type boundary, and ships an install command that places the set as Claude skills and as `AGENTS.md`. It depends on STORY-145 (the config semantics: intent, authorship, lifecycle, gates) and STORY-146 (the `config --json` the verbs read).

## Acceptance Criteria

- **Given** a config describing an arbitrary DAG (types, lifecycles, relations) that is not the default `RFC → Story → Iteration`,
  **When** a generic verb (`scaffold`, `co-write`, `generate`, `advance`, `execute`, `review`) is run for a type in that DAG,
  **Then** the verb reads the type and its rules from `config --json` at runtime and acts on the named type, with no DAG-specific type names baked into the skill prose.

- **Given** a type whose `authorship` ceiling is below the verb being requested (e.g. a `human` type asked to `generate`, or an `assisted` type asked to `generate`),
  **When** that above-ceiling authoring verb is invoked,
  **Then** the verb refuses and reports the ceiling, while a verb at or below the ceiling proceeds.

- **Given** a user positioned on a document whose next step would create a child of a different type,
  **When** the `/lazy` entry router runs,
  **Then** it locates the user in the DAG, advances only within the current document where eligible, and stops at the type boundary rather than auto-creating the child, leaving crossing to a human-initiated step.

- **Given** a project with no skills installed,
  **When** `lazyspec skills install` is run,
  **Then** the generic verb set is placed under `.claude/skills/` and concatenated into `AGENTS.md`, and `[skills] entry` is set to its default (`lazy`).

- **Given** a user who wants a different entry name,
  **When** `lazyspec skills install` is run with that entry configured,
  **Then** the router skill is installed under that name and `[skills] entry` records it, so invoking the custom name dispatches the router.

- **Given** a project that has never run `init`,
  **When** `lazyspec skills install` is run,
  **Then** the skills and `AGENTS.md` are placed successfully, confirming install is decoupled from `init`.

## Scope

### In Scope

- The static, portable, DAG-agnostic generic verb skill set, taking the type as a parameter and reading `config --json` at runtime (no baked type names):
  - `scaffold` / `co-write` / `generate` — authoring, ceiling-ordered; each honors the type's `authorship` ceiling and refuses an above-ceiling mode.
  - `advance` — transition status along the lifecycle DAG, maintain links, and check gates at the transition.
  - `execute` — carry out the work a delivery document describes.
  - `review` — critique a document or completed work against its intent and acceptance criteria.
  - `/lazy` — the entry router: reads config + status + context, locates the user in the DAG, dispatches a verb; advances within a document automatically but stops at type boundaries and never auto-crosses into a child type or from planning into building.
- Eligibility, authorship-ceiling, and gate logic expressed in skill prose (v1; no routing brain command in the binary).
- `lazyspec skills install [--runtime claude|agents-md]` — placement, not per-runtime transformation: place the one skill source as `.claude/skills/` for Claude and concatenate it into `AGENTS.md` for other agents; set `[skills] entry` (default `lazy`). Decoupled from `init`.
- The default skill set and a default `AGENTS.md` shipped with the tool.

### Out of Scope

- The config read/write CLI the skills consume (`config --json` and mutation subcommands) (STORY-146).
- The config axes themselves — intent, authorship, lifecycle, gates — and their enforcement (STORY-145).
- Enriched templates and `init` materialization, including the default *config* (STORY-145) and default *templates* (STORY-148); only the default *skills* and default *AGENTS.md* are in this slice.
- The `/configure-type` meta-skill (STORY-149).
