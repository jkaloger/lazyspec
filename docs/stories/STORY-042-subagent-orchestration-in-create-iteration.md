---
title: Subagent Orchestration in create-iteration
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags: [skills, subagents, orchestration]
related:
- implements: docs/rfcs/RFC-014-audit-skills-and-subagent-orchestration.md
---

## Context

When a story has multiple ACs that split naturally into separate iterations, `create-iteration` currently creates one iteration at a time. The same subagent dispatch pattern from `build` (and now `create-story`) applies: partition the ACs into iteration-sized groups upfront, then dispatch parallel subagents to create each iteration.

## Acceptance Criteria

- **AC-1:** **Given** a story with multiple ACs
  **When** `/create-iteration` is invoked
  **Then** the skill reads the story, groups ACs into iteration-sized chunks, and defines which ACs belong to each iteration

- **AC-2:** **Given** AC groups have been defined
  **When** the grouping is complete
  **Then** the skill presents the grouping to the user for approval before dispatching subagents

- **AC-3:** **Given** the user approves the grouping
  **When** subagents are dispatched
  **Then** each subagent receives the story context, its AC group, and the boundaries of other groups

- **AC-4:** **Given** N subagents are dispatched in parallel
  **When** all complete
  **Then** each AC belongs to exactly one iteration (no overlap) and each iteration links to the parent story via `implements`

- **AC-5:** **Given** all iterations have been created
  **When** the skill finishes
  **Then** `lazyspec validate` passes and the results are presented to the user

- **AC-6:** **Given** a story where all ACs fit in a single iteration
  **When** `/create-iteration` is invoked
  **Then** the skill creates the iteration directly without subagent dispatch

## Scope

### In Scope

- Updating `create-iteration` SKILL.md with subagent orchestration workflow
- Upfront partitioning of story ACs with user approval gate
- Parallel subagent dispatch with non-overlapping AC assignments
- Graceful fallback to single-iteration creation when ACs fit in one iteration

### Out of Scope

- Changes to the lazyspec CLI or engine
- Subagent orchestration in `create-story` (Story B)
- Changes to `plan-work` routing
- Changes to the `build` skill's existing orchestration
