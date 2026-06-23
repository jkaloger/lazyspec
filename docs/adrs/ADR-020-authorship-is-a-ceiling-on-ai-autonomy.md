---
title: Authorship is a ceiling on AI autonomy
type: adr
status: accepted
author: jkaloger
date: 2026-06-21
tags:
- skills
- authorship
- config
related:
- related-to: RFC-048
---

## Context

RFC-048 distinguishes human-written, AI-assisted, and AI-generated document types. A team needs "this type is human-written" to be an enforced property, not a hint an agent can talk itself past, while still letting a human choose a more manual mode on any type.

Authoring is three verbs, monotone in AI autonomy: `scaffold < co-write < generate`. The open question was whether the type's `authorship` field is a default (overridable upward), a lock (one permitted verb), or a ceiling.

## Decision

`authorship` is a ceiling on AI autonomy. The field names the maximum verb: `human` = scaffold only, `assisted` = up to co-write, `generated` = up to generate. A verb at or below the ceiling is always allowed; a verb above it is refused. A human can always drop to a more manual mode, never escalate past the configured ceiling.

Enforcement is ceiling-in-data: the ceiling is reported in `config --json` and honored by the skill, with a `validate` rule as a detective backstop. The binary cannot prevent over-ceiling authoring at the point of writing, because prose bodies are produced by file edits, not a CLI mutation.

## Consequences

- "Human-authored doc type" is a guarantee at the sanctioned path (the skill refuses, validate flags violations), matching the existing honor-system trust model for "don't edit files directly".
- The monotone ordering keeps the model simple: one field, three values, a total order.
- Not preventive against an agent that ignores the tooling. Accepted; same trust boundary as the rest of the skill rules.

Locked for RFC-048.
