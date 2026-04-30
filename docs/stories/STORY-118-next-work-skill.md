---
title: Next-work skill
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
priority: should
---



## Summary

A new `/next-work` skill that turns the `lazyspec next` query into a human-in-the-loop work-pickup flow. The skill queries `lazyspec next --json`, presents the candidate set with each candidate's `kind`, status, leased state, and the surrounding bottleneck context, asks the user which candidate to claim, calls `lazyspec claim`, then hands off based on candidate kind: `claimable` to `/build`, `needs-children` to `/create-story` or `/plan-work`, `needs-status-update` back to the user for a status bump.

## Scope

### In Scope

- Skill invokes `lazyspec next --json` and parses ready candidates plus bottlenecks plus warnings.
- Skill renders the candidate set to the user with `kind`, status, lease state, and bottleneck context.
- Skill prompts the user to pick a candidate (or decline).
- On selection, skill invokes `lazyspec claim` for the chosen candidate.
- Hand-off routing by candidate kind: `claimable` to `/build`; `needs-children` to `/create-story` (Story-level) or `/plan-work` (RFC-level); `needs-status-update` surfaced to the user with a prompt to advance status.
- Empty ready set: skill surfaces the bottleneck list as guidance instead of hanging or claiming nothing silently.

### Out of Scope

- Daemon-driven autonomous selection (RFC-036).
- The `/sequence` TUI skill (Story 5 of RFC-041).
- The `lazyspec next` CLI itself (Story 3, dependency).
- Engine graph primitives (Story 1).
- TUI sequencing screen (Stories 4a-c).
- Auto-bumping status without human confirmation.

## Acceptance Criteria

- **Given** a project where `lazyspec next --json` returns an empty `ready` array with non-empty `bottlenecks`,
  **When** the user invokes `/next-work`,
  **Then** the skill surfaces the bottlenecks as the reason no work is ready and exits without claiming anything.

- **Given** a project where `lazyspec next --json` returns multiple ready candidates,
  **When** the user invokes `/next-work`,
  **Then** the skill presents each candidate's id, `kind`, status, and lease state and prompts the user to pick one.

- **Given** the user has selected a candidate with `kind: claimable`,
  **When** the skill processes the selection,
  **Then** the skill calls `lazyspec claim` for that candidate and hands off to `/build`.

- **Given** the user has selected a candidate with `kind: needs-children`,
  **When** the skill processes the selection,
  **Then** the skill calls `lazyspec claim` and hands off to `/create-story` for a Story candidate or `/plan-work` for an RFC candidate.

- **Given** the user has selected a candidate with `kind: needs-status-update`,
  **When** the skill processes the selection,
  **Then** the skill surfaces the candidate to the user with a prompt to advance its status and does not hand off to `/build`.

- **Given** the user is shown a candidate set,
  **When** the user declines to pick any candidate,
  **Then** the skill exits cleanly without invoking `lazyspec claim` and without a hand-off.

- **Given** `lazyspec next --json` includes candidates with a `leased_by` field set,
  **When** the skill renders the candidate set,
  **Then** leased candidates are visually distinguished and identify the lessee.
