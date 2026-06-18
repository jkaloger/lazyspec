---
title: Per-type opt-in for agent mode, off by default
type: adr
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-046
---

## Context

Interactive agent mode is always available. Every document type offers the same fixed action set with no opt-in and no off switch, regardless of whether the project wants agents involved with that type.

## Decision

Gate agent mode per document type via an `agents: Vec<String>` field on `TypeDef`, defaulting to empty (`#[serde(default)]`). Entries are template file stems under `.lazyspec/agents/`. An absent or empty list is the valid "off" state for that type, not a load error. The action set for a document is its type's `agents` list intersected with the templates that actually loaded; a named-but-missing template is reported. Action gating is per type and `allowed_tools` is per template (frontmatter); neither is global. The one global `[agents]` key is the interactive launch command (ADR-017), which selects which tool a terminal-handover session runs -- a machine/project property, not which actions a type exposes.

Rejected: a global `[agents]` block as the gating mechanism plus a per-type allowlist (two-level config for one concern). Rejected: always-on with no gating (no opt-in, the status quo being removed).

## Consequences

- Agent mode is off by default; a type opts in explicitly by listing template stems.
- Tool scope is authored alongside each prompt rather than centrally configured.
- Adding or removing an action for a type is a config edit, not a code change.

