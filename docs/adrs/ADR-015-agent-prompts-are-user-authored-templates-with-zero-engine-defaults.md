---
title: Agent prompts are user-authored templates with zero engine defaults
type: adr
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-046
---

## Context

Interactive agent prompts are hardcoded in Rust: `build_expand_prompt` and `build_create_children_prompt` in `src/tui/agent.rs`. A project cannot change what an agent is told without recompiling lazyspec. This contradicts the strict-config / no-engine-defaults stance of ADR-011 and the unopinionated-taxonomy direction of RFC-042, both of which hold that the engine carries no default ontology and config is the sole source.

## Decision

Agent prompts are user-authored templates at `.lazyspec/agents/<name>.md` (YAML frontmatter plus a minijinja body rendered in strict-undefined mode). The engine ships zero default prompts and `init` writes none. With no template files and no per-type config, agent mode does not appear -- there is no embedded prompt to fall back to. The hardcoded `build_expand_prompt` / `build_create_children_prompt` are deleted; their capabilities (expanding a document, deriving children) become templates a project authors if it wants them, using the `document.*` and `child_types` render variables.

Rejected: `init` writes starter templates (still an opinion the engine owns about what agents should do). Rejected: embedded fallback prompts overridable by file (a baked default, the exact thing ADR-011 removes).

## Consequences

- Agent behaviour is owned entirely by the project; there are no surprise built-in actions.
- A fresh project has agent mode off until it authors templates and opts a type in.
- STORY-053 (custom agent prompts) is subsumed: there is no separate "custom" path; all prompts are templates.

