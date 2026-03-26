---
title: Agent Workflow Skills
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- skills
- agents
- superpowers
related:
- implements: RFC-002
---


## Context

AI agents need structured guidance on how to use lazyspec within the development workflow. Superpowers skill files provide step-by-step workflows with d2 diagrams that agents invoke at the appropriate trigger points.

## Acceptance Criteria

### AC1: write-rfc skill

**Given** an agent needs to propose a design or significant change
**When** the `write-rfc` skill is invoked
**Then** the skill guides the agent through creating an RFC with intent, interface sketches, and story identification

### AC2: create-story skill

**Given** an agent is starting a new feature or card
**When** the `create-story` skill is invoked
**Then** the skill guides the agent through creating a Story with given/when/then ACs linked to a parent RFC

### AC3: create-iteration skill

**Given** an agent is implementing against a Story
**When** the `create-iteration` skill is invoked
**Then** the skill guides the agent through creating an Iteration linked to the Story, with TDD (tests before implementation)

### AC4: resolve-context skill

**Given** an agent needs full context before beginning work
**When** the `resolve-context` skill is invoked
**Then** the skill guides the agent through walking the RFC -> Story -> Iteration chain using lazyspec commands

### AC5: review-iteration skill

**Given** an iteration is complete and ready for review
**When** the `review-iteration` skill is invoked
**Then** the skill enforces two-stage review: AC compliance first, code quality second, blocking on AC failure

### AC6: Skill file format

**Given** each skill file in `skills/`
**When** the file is read
**Then** it has YAML frontmatter with `name` and `description`, a d2 workflow diagram, numbered steps, and rules

## Scope

### In Scope

- Five skill files: write-rfc, create-story, create-iteration, resolve-context, review-iteration
- d2 workflow diagrams
- Step-by-step instructions using lazyspec CLI commands
- Rules section per skill

### Out of Scope

- Skill execution runtime
- Skill discovery or registration mechanism
- Integration testing of skills with actual agents
