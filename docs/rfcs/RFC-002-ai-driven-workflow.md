---
title: AI-Driven Development Workflow
type: rfc
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- ai
- workflow
- agents
- skills
related:
- supersedes: RFC-001
---


## Summary

Extend lazyspec to support an agent-driven development workflow by renaming document types to match the development lifecycle (RFC -> Story -> Iteration), adding full-text search, strict validation, and Superpowers skill files that guide AI agents through the workflow.

## Problem

The original Spec/Plan naming doesn't convey the relationship between design intent and implementation work. Agents need structured guidance on how to use lazyspec, and the validation rules need to enforce the document hierarchy (every iteration must link to a story, every ADR must have at least one relation).

## Design Intent

### Document Type Rename

| Before | After | Purpose |
|--------|-------|---------|
| Spec | Story | Acceptance criteria for a vertical slice of work |
| Plan | Iteration | Implementation log against a Story |
| RFC | RFC (unchanged) | Design intent and problem framing |
| ADR | ADR (unchanged) | Architectural decision record |

The hierarchy becomes: RFC (intent) -> Story (acceptance criteria) -> Iteration (implementation). Each level links to the one above via `implements`.

### Story Template

Stories use given/when/then acceptance criteria. Each AC must be independently testable and readable by a non-technical stakeholder.

### Iteration Template

Iterations document changes made, test plans, and notes. They must link to a parent Story.

### Search

New `lazyspec search <query>` command. Case-insensitive substring matching across titles, tags, and body content. Supports `--type` filtering and `--json` output for agent consumption.

### Strict Validation

All validation is strict. No flags to weaken it.

| Check | Rule |
|-------|------|
| Broken links | All `related` targets must resolve |
| Iteration linkage | Every iteration must `implement` a story |
| ADR linkage | Every ADR must have at least one relation |

### Agent Skills

Five Superpowers skill files in `skills/`:

- `write-rfc` - proposing designs
- `create-story` - starting features with ACs
- `create-iteration` - TDD implementation against a story
- `resolve-context` - gathering the RFC->Story->Iteration chain
- `review-iteration` - two-stage review (AC compliance, then code quality)

## Stories

1. Story and Iteration document types (rename, templates, search command, strict validation)
2. Agent workflow skills (five skill files)
