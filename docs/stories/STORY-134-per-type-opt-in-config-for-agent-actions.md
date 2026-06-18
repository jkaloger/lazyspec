---
title: Per-type opt-in config for agent actions
type: story
status: accepted
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-046
---

## Context

Agent mode is always on: every document type offers the same fixed action set whether or not the project wants agents involved with that type. Per ADR-016, gating moves per type via an `agents: Vec<String>` field on `TypeDef`, off by default. The entries are template file stems under `.lazyspec/agents/`, and an absent or empty list is the valid "off" state, not a load error -- consistent with the strict, no-engine-defaults stance of ADR-011 (the field's shape is validated, but no built-in ontology is injected). This story adds the field and the resolution from a document's type to its allowed action set; it does not load, render, or present the templates.

## Acceptance Criteria

### AC1: Absent agents key means off and is not an error

**Given** a `[[types]]` entry with no `agents` key
**When** the config is loaded
**Then** the load succeeds (no error) and the type resolves to an empty action set (agent mode off for that type)

### AC2: Empty agents list means off

**Given** a `[[types]]` entry declaring `agents = []`
**When** the config is loaded
**Then** the load succeeds and the type resolves to an empty action set (agent mode off for that type)

### AC3: Listed stems resolve to that action set

**Given** a type declares `agents = ["expand", "create-children"]` and both `expand` and `create-children` templates loaded
**When** the action set is resolved for a document of that type
**Then** the action set is exactly those two loaded templates (the type's list intersected with the loaded templates)

### AC4: A listed-but-missing template is reported

**Given** a type declares `agents = ["expand", "nonexistent"]` and only `expand` loaded
**When** the action set is resolved for a document of that type
**Then** the resolved action set contains only `expand`, and `nonexistent` is reported as named-but-missing (the user named an action they did not author)

### AC5: An unreferenced template is unused, not an error

**Given** a template file exists under `.lazyspec/agents/` that no type's `agents` list references
**When** the config is loaded and action sets are resolved
**Then** no error is raised and that template simply does not appear in any type's action set

### AC6: Resolution is per type

**Given** one type lists agents and another type has no `agents` key
**When** action sets are resolved for documents of each type
**Then** the listing type yields its intersected action set and the other type yields an empty action set, independently of each other

## Scope

### In Scope

- Add `agents: Vec<String>` to `TypeDef`, defaulting to empty via `#[serde(default)]`; entries are template file stems under `.lazyspec/agents/`
- Treat an absent or empty `agents` key as the valid "off" state for the type (validate the field's shape only; do not error on empty/absent)
- Resolution from a document's type to its action set: the type's `agents` list intersected with the set of templates that actually loaded
- Report names listed in `agents` with no matching loaded template (named-but-missing)
- Treat a loaded template not referenced by any type's `agents` list as unused (not an error)

### Out of Scope

- The `AgentRunner` trait and headless spawning, `AgentContext` / `AgentHandle` (Story 1)
- Discovering, parsing, and rendering the template files; this story consumes the set of loaded template names and does not load them (Story 2)
- The TUI dialog that presents the resolved actions (Story 4)
- Interactive run mode and the global `[agents]` block / `interactive` command; this story concerns the per-type `[[types]].agents` list only, which is distinct from the global `[agents] interactive` key (Story 5)
