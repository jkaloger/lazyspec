---
title: Subagent Orchestration in create-story
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags: [skills, subagents, orchestration]
related:
- implements: docs/rfcs/RFC-014-audit-skills-and-subagent-orchestration.md
---

## Context

When an RFC identifies multiple vertical slices, `create-story` currently creates one story at a time. This means N sequential invocations, each losing context from the previous. The `build` skill already demonstrates the subagent dispatch pattern (one subagent per task). The same pattern applies here: `create-story` should partition the slices upfront and dispatch parallel subagents.

## Acceptance Criteria

- **AC-1:** **Given** an RFC that identifies multiple vertical slices
  **When** `/create-story` is invoked
  **Then** the skill reads the RFC, extracts the identified slices, and defines non-overlapping scope boundaries for each

- **AC-2:** **Given** scope boundaries have been defined for N slices
  **When** the partitioning is complete
  **Then** the skill presents the partition to the user for approval before dispatching subagents

- **AC-3:** **Given** the user approves the partition
  **When** subagents are dispatched
  **Then** each subagent receives the RFC context, its specific slice definition, and the scope boundaries of adjacent slices

- **AC-4:** **Given** N subagents are dispatched in parallel
  **When** all complete
  **Then** each created story has non-overlapping scope and links to the parent RFC via `implements`

- **AC-5:** **Given** all stories have been created
  **When** the skill finishes
  **Then** `lazyspec validate` passes and the results are presented to the user

- **AC-6:** **Given** an RFC that identifies only one slice
  **When** `/create-story` is invoked
  **Then** the skill creates the story directly without subagent dispatch (no unnecessary overhead)

## Scope

### In Scope

- Updating `create-story` SKILL.md with subagent orchestration workflow
- Upfront partitioning of RFC slices with user approval gate
- Parallel subagent dispatch with non-overlapping scope definitions
- Graceful fallback to single-story creation when only one slice exists

### Out of Scope

- Changes to the lazyspec CLI or engine
- Subagent orchestration in `create-iteration` (Story C)
- Changes to `plan-work` routing
- Runtime coordination or locking between subagents
