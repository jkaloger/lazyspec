---
title: Customize the comment vocabulary for my project
type: story
status: rejected
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a project maintainer, I want to define my own comment vocabulary in config, so that comments carry the semantics my domain needs (`severity`, `tags`, `resolves`, …) instead of a fixed social-feature schema.

Mirrors how document `[[types]]` are configured: the engine ships a default schema and hard-codes none of it.

## Scope

- `[comments.attributes]` config block declaring each attribute: `type` (`enum` / `number` / `string` / `list`), allowed values or range, `default`, `required`.
- Project overrides the shipped default schema wholesale (rename, drop, constrain, add).
- The authored-attribute validation from the prior story reads this schema; defaults apply when unset.

Out of scope: resolution predicate config (next story); adapter-sourced attributes.

## Acceptance Criteria

- **Given** no `[comments.attributes]` in config, **when** I post a comment, **then** the shipped default schema (`kind`, `confidence`, `anchor`) is in force.
- **Given** a config declaring `severity` as an enum `[low, medium, high]`, **when** I run `comment add --attr severity=high`, **then** it validates and stores; `--attr severity=urgent` is rejected.
- **Given** a config that drops `confidence`, **when** I run `comment add --attr confidence=0.7`, **then** it is rejected as unknown.
- **Given** an attribute declared `required`, **when** I post without it, **then** the command fails naming the missing attribute.
- **Given** an attribute declared with a `default`, **when** I post without it, **then** the comment records the default value.

## Notes

Consistent with document-type configurability — not novel surface area. The engine never relies on `kind`/`confidence` existing.

