---
title: Configure-type meta-skill
type: story
status: draft
author: jkaloger
date: 2026-06-21
tags: []
related:
- implements: RFC-048
---

## Context

RFC-048 makes the workflow config-driven so the binary owns data and the skill owns prose. For a user's arbitrary type, neither a generic skill nor a code generator can invent authoring methodology the engine does not ship — ADR-011 forbids baked-in knowledge. The only source of per-type methodology is config data the user wrote.

So this slice adds a `/configure-type` meta-skill: a grill-me-style interview that co-authors that methodology with the user. The user supplies the knowledge; the skill extracts it and records it. It interviews for one type at a time, asking a single question at a time and recommending an answer, covering the type's name, `intent`, `authorship`, lifecycle (`states` + `edges`), status gates (`require_parent_status`), and relations to existing types.

It then writes the result two ways: an enriched per-type template (STORY-148 format) and the `[[types]]` config block with its relations and gates. It writes the config through the config-write CLI (STORY-146: `config add-type`, `set-lifecycle`, `add-gate`) — never by hand-editing `.lazyspec.toml`. Default types ship pre-configured, so most users never run this. Greenfield setup is running the skill once per type; a whole-DAG bootstrap wizard is future work, not this story.

## Acceptance Criteria

- **Given** a user invokes `/configure-type` for a new custom type
  **When** the interview runs
  **Then** it asks one question at a time, recommending an answer, and elicits the type's name, `intent`, `authorship` (human/assisted/generated), lifecycle (`states` and `edges`), status gates (`require_parent_status`), and relations to existing types

- **Given** the interview has gathered the type's axes
  **When** the skill records the methodology
  **Then** it produces an enriched per-type template (STORY-148 format) for that type

- **Given** the interview has gathered the type's axes
  **When** the skill writes the config
  **Then** it adds the `[[types]]` block, relations, and gates by calling the config-write CLI (`config add-type`, `set-lifecycle`, `add-gate`) and never hand-edits `.lazyspec.toml`

- **Given** a custom type has been configured via the skill
  **When** `lazyspec config --json` runs
  **Then** the new type appears with its `intent`, `authorship`, lifecycle, and gates populated as supplied in the interview

- **Given** a user runs `/configure-type`
  **When** the interview completes
  **Then** exactly one type is configured per run, and configuring more types means running the skill again

## Scope

### In Scope

- A `/configure-type` skill: a grill-me-style interview for one document type, asking one question at a time and recommending answers
- Eliciting the type's name, `intent`, `authorship` (human/assisted/generated), lifecycle (`states` + `edges`), status gates (`require_parent_status`), and relations to existing types
- Producing the enriched per-type template (STORY-148 format) for the configured type
- Writing the `[[types]]` block, relations, and gates via the config-write CLI (`config add-type`, `set-lifecycle`, `add-gate`) rather than hand-editing TOML
- Greenfield setup: run the skill once per type

### Out of Scope

- The enriched template format itself (STORY-148)
- The config-write CLI implementation — `config add-type`, `set-lifecycle`, `add-gate` (STORY-146)
- The generic verb skills (STORY-147)
- The config axes and schema — `intent`, `authorship`, `lifecycle`, `require_parent_status` (STORY-145)
- A whole-DAG bootstrap wizard that composes `/configure-type` with a DAG-design step (future)
