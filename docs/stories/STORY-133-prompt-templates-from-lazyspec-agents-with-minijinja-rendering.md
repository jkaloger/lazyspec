---
title: Prompt templates from .lazyspec/agents with minijinja rendering
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-046
- supersedes: STORY-053
---

## Context

Agent prompts are baked into the Rust binary as `build_expand_prompt` and `build_create_children_prompt` in `src/tui/agent.rs`, so a project cannot change what an agent is told without recompiling lazyspec. RFC-046 and ADR-015 replace these with user-authored prompt templates discovered under `.lazyspec/agents/`, rendered with minijinja against document context. This slice owns discovery, frontmatter parsing into an `AgentPrompt` model, and strict-undefined rendering; the engine ships zero default prompts (zero-defaults principle), so the hardcoded builders are deleted with no embedded fallback. With no template files, agent mode simply has nothing to offer.

## Acceptance Criteria

### AC1: Template discovery

**Given** one or more `*.md` files exist under `.lazyspec/agents/`
**When** prompt discovery runs
**Then** each file with valid frontmatter is loaded and available as an `AgentPrompt`

### AC2: Valid frontmatter parse

**Given** a template file whose frontmatter sets `name`, `description`, `mode`, and `allowed_tools`
**When** the file is parsed
**Then** an `AgentPrompt` is produced with those values and its markdown body retained as the render template

### AC3: Mode defaults to headless

**Given** a template file whose frontmatter omits `mode`
**When** the file is parsed
**Then** the resulting `AgentPrompt` has `mode` of `headless`

### AC4: Strict-undefined render success

**Given** a valid template whose body references only known variables (`document.*` and/or `child_types`)
**When** the template is rendered against a selected document
**Then** a fully rendered prompt string is returned with each variable substituted from the document context

### AC5: Document context exposed

**Given** a template body referencing `document.id`, `document.title`, `document.type`, `document.body`, `document.status`, and `document.path`
**When** the template is rendered against a selected document
**Then** each field resolves to that document's corresponding value

### AC6: child_types context exposed

**Given** a template body referencing `child_types` for a document whose type has child types in the parent-child config rules
**When** the template is rendered
**Then** `child_types` resolves to the list of child type names for that document's type

### AC7: Unknown-variable render error

**Given** a template body referencing a variable not present in the render context
**When** the template is rendered
**Then** rendering fails with an error surfaced to the user, rather than substituting a silent empty string

### AC8: Malformed file skip-and-warn

**Given** a file under `.lazyspec/agents/` with missing or malformed frontmatter (e.g. absent `name` or `description`)
**When** discovery runs
**Then** the file is skipped with a warning and does not appear as an available prompt

### AC9: No built-in prompts remain

**Given** the engine after this change
**When** the codebase is inspected
**Then** `build_expand_prompt` and `build_create_children_prompt` no longer exist and no default prompt is embedded or written by `init`

### AC10: context lineage exposed

**Given** a template body referencing `context.ancestors` and `context.related` for a document that implements a parent and links to other documents
**When** the template is rendered against that document
**Then** `context.ancestors` resolves to the document's `implements` chain (nearest parent first) and `context.related` to its adjacent `related-to` documents, each entry exposing the same `document.*` fields, sourced from `resolve_chain` rather than a re-derived DAG

## Scope

### In Scope

- Discovering `*.md` template files under `.lazyspec/agents/`
- Parsing YAML frontmatter into an `AgentPrompt`: required `name` and `description`; optional `mode` (`headless` | `interactive`, default `headless`) and optional `allowed_tools`
- Rendering the markdown body with minijinja in strict-undefined mode
- Render context exposing `document` (`id`, `title`, `type`, `body`, `status`, `path`), `child_types` (child type names for the document's type, derived from the parent-child config rules), and `context` (resolved lineage from `resolve_chain`: `context.ancestors` = the `implements` chain, `context.related` = adjacent `related-to` documents, each entry with the same `document.*` fields)
- Surfacing an unknown-variable reference as an error rather than an empty string
- Skipping files with missing/malformed frontmatter, with a warning, so they are not offered
- Deleting `build_expand_prompt` / `build_create_children_prompt`; the engine ships zero default prompts

### Out of Scope

- The `AgentRunner` trait, `ClaudeP`, and spawning processes (STORY-132 / slice 1)
- Per-type config gating via `[[types]].agents` — this slice loads and renders templates; it does not decide which types may use them (slice 3)
- The TUI action dialog that presents and selects templates (slice 4)
- Acting on `mode: interactive` (terminal handover, the `[agents]` interactive command, suspend/run/restore) — this slice only parses `mode` into `AgentPrompt` (slice 5)
- Headless run history relocation and the per-type opt-in resolution rules
