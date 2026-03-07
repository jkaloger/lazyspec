---
title: Create-Audit Skill
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags: [skills, audits]
related:
- implements: docs/rfcs/RFC-014-audit-skills-and-subagent-orchestration.md
---

## Context

The `audit` document type is registered in `.lazyspec.toml` but has no skill to drive it. Audits are criteria-based reviews (health checks, security, accessibility, pen tests, bug bashes, spec compliance) that produce findings the user triages into iterations. They sit outside the main RFC -> Story -> Iteration pipeline.

## Acceptance Criteria

- **AC-1:** **Given** a `skills/create-audit/SKILL.md` file exists
  **When** an agent invokes `/create-audit`
  **Then** the skill guides the agent through audit creation with scope definition, criteria selection, finding documentation, and user presentation

- **AC-2:** **Given** a generic audit template exists at `.lazyspec/templates/audit.md`
  **When** `lazyspec create audit` is run
  **Then** the created document has sections for scope, criteria, findings (with severity), and summary

- **AC-3:** **Given** an audit has been created
  **When** the agent documents findings
  **Then** each finding has a severity rating (critical, high, medium, low, info), a location, a description, and a recommendation

- **AC-4:** **Given** an audit with findings exists
  **When** the skill completes
  **Then** findings are presented to the user for triage before any iterations are created

- **AC-5:** **Given** an audit is related to existing stories
  **When** the audit is created
  **Then** the skill links the audit to those stories using `related-to` relationships

## Scope

### In Scope

- `create-audit` SKILL.md following existing skill conventions
- Generic audit template in `.lazyspec/templates/`
- Audit lifecycle: create, review codebase, document findings, present to user
- Linking audits to existing stories/RFCs via `related-to`

### Out of Scope

- Audit-type-specific templates (security, accessibility, etc.) -- future work
- Automatic iteration creation from findings -- user triages manually
- Changes to the lazyspec CLI or engine
- Updates to `plan-work` routing for audits
