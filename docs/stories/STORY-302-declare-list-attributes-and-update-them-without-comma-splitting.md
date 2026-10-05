---
title: Declare list attributes and update them without comma splitting
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-049
---

## Value

As a pack author, I want a `list` attribute kind, so a document can carry several values (reviewers, components) with first-class validation instead of an undeclared comma string.

## Acceptance Criteria

- AC1: `[[types]].attributes` accepts `kind = "list"`. Frontmatter holds a YAML sequence of strings.
- AC2: A declared `list` attribute does not raise `undeclared attribute` (`src/engine/validation.rs:1721`). A non-sequence value is a type-mismatch error like other kinds.
- AC3: `update` accepts repeatable `--attr name+=value` (append, no duplicate), `--attr name-=value` (remove), and `--attr name=` (clear to empty list). `name=value` on a list sets a single-item list. Values are never split on commas.
- AC4: `config` attribute specs accept `NAME:list[:required]` (`parse_attr_spec`, `src/cli/config.rs:1155`) and round-trip through `config_write` (`src/engine/config_write.rs:205`).
- AC5: `show --json`, `list` columns and the TUI render a list attribute as its items.
- AC6: `+=`/`-=` on a non-list attribute errors.

## Scope

### In Scope

- `AttrKind::List` (`src/engine/config.rs:717`), validation, `parse_attr_pairs` (`src/cli/update.rs:146`), config CLI, JSON schema.

### Out of Scope

- Lists of non-strings.
- `enum` constrained list items (candidate follow-up).
