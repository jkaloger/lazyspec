---
title: Tag a comment with project vocabulary
type: story
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a reviewer, I want to tag a comment with project vocabulary (kind, confidence, anchor, …), so that a note carries machine-readable semantics — an observation, a challenge, a decision — not just prose.

Adds the authored-attribute path against the shipped default schema. (Customizing the schema is the next story; this one ships and validates against the defaults.)

## Scope

- `lazyspec comment add <doc> --attr key=value` — repeatable, generic; no per-attribute flag compiled in.
- Validation of each pair against the shipped default schema (`kind` enum, `confidence` number, `anchor` string).
- Valid attributes stored in the comment's `attributes` map; surfaced in `--json` and as chips in pretty output.
- Invalid pairs rejected with a `--json` diagnostic (unknown key, bad type, out-of-range value).

Out of scope: overriding the schema (next story); resolution predicate; remote-sourced attributes.

## Acceptance Criteria

- **Given** the default schema, **when** I run `comment add <doc> --attr kind=observation --attr confidence=0.7`, **then** the comment stores `kind=observation` and `confidence=0.7` and they appear in `--json` and as chips in pretty output.
- **Given** `--attr kind=nonsense` where `kind` is an enum without that value, **then** the command fails with an out-of-range diagnostic and posts nothing.
- **Given** `--attr confidence=high` where `confidence` is a number, **then** the command fails with a type diagnostic.
- **Given** `--attr severity=high` where no `severity` attribute is declared, **then** the authored attribute is rejected as unknown.
- **Given** `--attr anchor="#design"`, **then** the comment records the section slug (slug, not line — lines drift on edit).

## Notes

Adding a new attribute needs only a config change, never a CLI change — `--attr` is generic. Validation applies only on the authored path.

