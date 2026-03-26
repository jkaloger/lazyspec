---
title: AI-Driven Workflow Design
type: iteration
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- design
- ai-workflow
related:
- implements: STORY-004
- implements: STORY-005
---


# AI-Driven Development Workflow Design

> Extends lazyspec with Story/Iteration document types, full-text search, strict validation, and Superpowers skill files to support agent-driven development workflows.

## Summary

Replace the existing SPEC and PLAN document types with STORY and ITERATION to establish a clear hierarchy: RFC -> STORY -> ITERATION. Add a `search` command for cross-document discovery, make validation always strict, and ship Superpowers skill files that guide agents through the workflow.

## Document Type Changes

### Before -> After

| Before | After | Directory |
|--------|-------|-----------|
| `DocType::Spec` | `DocType::Story` | `docs/stories` |
| `DocType::Plan` | `DocType::Iteration` | `docs/iterations` |
| `DocType::Rfc` | unchanged | `docs/rfcs` |
| `DocType::Adr` | unchanged | `docs/adrs` |

This is a breaking change. Existing documents with `type: spec` or `type: plan` will fail to parse. The config file changes from `specs`/`plans` to `stories`/`iterations`.

### Story-Iteration Linkage

Stories and iterations are linked via the existing `related` frontmatter using the `implements` relation type:

```yaml
# Iteration frontmatter
related:
  - implements: docs/stories/STORY-001-user-auth.md
```

```yaml
# Story frontmatter (linking to parent RFC)
related:
  - implements: docs/rfcs/RFC-001-auth-design.md
```

No new relationship types needed. The hierarchy is expressed through existing `implements` relations.

## Templates

### Story Template

```markdown
---
title: "{title}"
type: story
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Context

## Acceptance Criteria

### AC1: <criteria name>

**Given** <precondition>
**When** <action>
**Then** <expected outcome>

## Scope

### In Scope
-

### Out of Scope
-
```

### Iteration Template

```markdown
---
title: "{title}"
type: iteration
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Changes

## Test Plan

## Notes
```

## Search Command

New CLI subcommand: `lazyspec search <query> [--type <type>] [--json]`

Searches across:
- Document titles (frontmatter `title` field)
- Tags
- Document body content (full-text)

Uses substring matching. Returns matching documents with path, title, type, status, and a content snippet showing the match context. Supports `--json` for agent consumption.

### Implementation

Add a `search` method to `Store` that:
1. Iterates all documents
2. Checks title, tags, and body content against the query (case-insensitive)
3. Returns matches with the matching field and a snippet

## Validation (Always Strict)

Remove the distinction between normal and strict validation. The `validate` command always runs full relational integrity checks:

| Check | Description |
|-------|-------------|
| Broken links | All `related` targets must resolve to existing documents (existing) |
| Iteration linkage | Every `iteration` must have an `implements` relation to a `story` |
| ADR linkage | Every `adr` must have at least one relation to another document |
| Story completeness | Stories in `accepted` status must have all related refs resolvable |

Each check produces a `ValidationError` variant. The `--strict` flag is removed.

## Superpowers Skill Files

Five skill files in `skills/` at the project root:

### `create-story`
- **Trigger:** Starting a new feature or card
- **Workflow:** Guide agent through `lazyspec create story`, setting up given/when/then ACs, linking to RFC

### `create-iteration`
- **Trigger:** Implementing against a Story
- **Workflow:** Guide agent through creating iteration with `implements` relation to story, TDD approach (tests before implementation)

### `resolve-context`
- **Trigger:** Agent needs context before beginning work
- **Workflow:** Instruct agent to use `lazyspec show` and `lazyspec list` to gather full context chain

### `review-iteration`
- **Trigger:** Iteration complete, before merge
- **Workflow:** Two-stage review: (1) verify all Story ACs are satisfied, (2) code quality review. Block on AC failure before reviewing code.

### `write-rfc`
- **Trigger:** Proposing a design or significant change
- **Workflow:** Guide RFC creation with intent, interface sketches, story identification

## Files Changed

### Engine
- `src/engine/document.rs` — `DocType` enum: `Spec` -> `Story`, `Plan` -> `Iteration`
- `src/engine/config.rs` — `Directories` struct: `specs` -> `stories`, `plans` -> `iterations`
- `src/engine/store.rs` — new `search` method, expanded `validate` with strict checks

### CLI
- `src/cli/mod.rs` — add `Search` variant, update doc type help text
- `src/cli/search.rs` — new file, search command implementation
- `src/cli/validate.rs` — updated for new validation error variants
- `src/cli/create.rs` — update accepted type names
- `src/cli/init.rs` — create `stories` and `iterations` directories

### TUI
- `src/tui/app.rs` — update type labels from SPEC/PLAN to STORY/ITERATION
- `src/tui/ui.rs` — update display strings

### Templates
- `.lazyspec/templates/story.md` — new template
- `.lazyspec/templates/iteration.md` — new template
- Remove `spec.md` and `plan.md` templates if they exist

### Skills
- `skills/create-story.md`
- `skills/create-iteration.md`
- `skills/resolve-context.md`
- `skills/review-iteration.md`
- `skills/write-rfc.md`

### Tests
- Update all existing tests referencing `spec`/`plan` to `story`/`iteration`
- Add tests for search command
- Add tests for new validation rules

### Config
- `.lazyspec.toml` — update directory names
