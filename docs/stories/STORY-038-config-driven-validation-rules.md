---
title: Config-driven validation rules
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags: []
related:
- implements: RFC-013
---




## Context

Validation rules in `validation.rs` are hardcoded against specific `DocType` variants (e.g. `DocType::Iteration` must implement `DocType::Story`). With custom types, these rules need to come from config. RFC-013 introduces a `[[rules]]` config array with two rule shapes: parent-child link requirements and relation-existence requirements.

## Acceptance Criteria

- **Given** a `.lazyspec.toml` with no `[[rules]]` section
  **When** validation runs
  **Then** the default rules apply (iterations must implement stories, ADRs must have relations)

- **Given** a `[[rules]]` section with a parent-child rule (`child`, `parent`, `link`, `severity`)
  **When** a document of the child type exists without the required link to a parent type
  **Then** a validation issue is raised at the configured severity level

- **Given** a `[[rules]]` section with a relation-existence rule (`type`, `require = "any-relation"`, `severity`)
  **When** a document of that type exists with no relations
  **Then** a validation issue is raised at the configured severity level

- **Given** a user provides `[[rules]]` in config
  **When** validation runs
  **Then** only the user-provided rules are evaluated (defaults are replaced, not merged)

- **Given** custom types with parent-child rules configured
  **When** a child document implements a rejected or superseded parent
  **Then** the existing status-based validation still fires (inferred from the configured hierarchy)

- **Given** a `[[rules]]` entry with an invalid severity value
  **When** the config is parsed
  **Then** a clear error is returned

## Scope

### In Scope

- `[[rules]]` config parsing for both rule shapes
- Default rules matching current validation behavior
- Parent-child rules with configurable child, parent, link type, and severity
- Relation-existence rules with configurable type and severity
- Status-based validation infers hierarchy from configured parent-child rules

### Out of Scope

- Type definitions (covered by STORY-037)
- CLI propagation (covered by STORY-039)
- Custom rule shapes beyond parent-child and relation-existence
- Workflow enforcement rules (e.g. status transitions)
