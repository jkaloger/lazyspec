---
title: Config axes, status DAG, and transition enforcement
type: story
status: accepted
author: jkaloger
date: 2026-06-21
tags: []
related:
- implements: RFC-048
---

## Context

Today every document type shares one closed, hard-coded set of statuses, and any status may follow any other. RFC-048 makes the workflow config-driven, but that depends on the config and engine layer describing each type's purpose, its authorship ceiling, and its own status lifecycle. This Story is the foundation: it adds those per-type axes, turns status into a value validated against a per-type lifecycle, and enforces both legal transitions and status-conditioned creation gates. Stories 146-149 build CLI, skills, templates, and meta-skills on top of what this slice establishes.

## Acceptance Criteria

- **Given** a config whose type declares `intent`, `authorship`, and a `lifecycle` (states plus edges),
  **When** the config is loaded,
  **Then** all three axes parse successfully and are readable for that type.

- **Given** a config type that declares no `authorship`,
  **When** the config is loaded,
  **Then** that type's authorship defaults to Assisted.

- **Given** a type whose lifecycle lists a fixed set of states,
  **When** a document of that type is loaded or its status is set,
  **Then** a status naming one of those states is accepted and a status outside that set is rejected.

- **Given** a document at status A and a type lifecycle without an edge from A to B,
  **When** `update --status B` is run on that document,
  **Then** the transition is rejected and the status is unchanged; a transition along a declared edge succeeds.

- **Given** a parent-child rule that requires the parent to reach a named status before a child exists, and a parent that has not yet reached it,
  **When** `create <child>` is run against that parent,
  **Then** creation is refused; once the parent reaches the required status, the same `create` succeeds.

- **Given** a pre-existing config that has no lifecycle on its types,
  **When** `fix --config` is run,
  **Then** a default lifecycle (the prior seven statuses plus sensible edges) is injected so every type has a valid lifecycle.

## Scope

### In Scope

- Per-type config axes on each type: `intent` (one line on why the type exists), `authorship` (autonomy ceiling: Human, Assisted, or Generated; default Assisted), and `lifecycle` (an inline status DAG of states and edges, with `*` permitted as an edge source).
- Status as a value validated against the owning type's lifecycle states, replacing the closed seven-variant status set (mirrors the existing validated-relation-type pattern).
- `update --status` enforcing legal transitions: a status move is allowed only when a matching edge exists in the type's lifecycle.
- A status-conditioned creation gate via `require_parent_status` on the parent-child rule: `create <child>` is refused until the parent reaches the named status.
- A `fix --config` migration that injects a default lifecycle (the current seven statuses plus sensible edges) into any pre-existing config whose types lack one.
- The starter config shipping a default lifecycle for the default types.

### Out of Scope

- `config --json` read and the config-mutation CLI (STORY-146).
- Generic verb skills, their install, and AGENTS.md wiring (STORY-147).
- Enriched templates and init materialization (STORY-148).
- The `/configure-type` meta-skill (STORY-149).
