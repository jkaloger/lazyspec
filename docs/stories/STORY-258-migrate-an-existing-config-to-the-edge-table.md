---
title: Migrate an existing config to the edge table
type: story
status: in-progress
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
- blocks: STORY-259
---
As a maintainer of a project on an older config, I want `fix --config` to translate my rules and relationship traversal into edges, so that upgrading changes nothing about what validates.

ADR-012 set the precedent: a lenient read that bypasses strict load, with the strict-load error naming the remedy. This migration cannot inherit the append-only contract at `src/engine/ops/fix/config.rs:34-41`, because the source blocks must be removed or the config declares its DAG twice.

## Acceptance criteria

- Given a config with `[[rules]]` and relationship `traversal` keys, when `fix --config` runs, then `[[edges]]` is written and the source `[[rules]]` blocks and `traversal` keys are removed.
- Given a `parent-child` rule, when translated, then one edge is emitted per chain-marked relationship, each carrying `from = child`, `to = [parent]`, `via = ` that relationship's name, `traversal = "chain"`, and `required = severity`. Naming the relationship rather than writing `via = "*"` is the preservation: `validation.rs:583` satisfies the rule only through a chain-marked relationship, so the wildcard would widen it — and a `via = "*"` row carrying `traversal = "chain"` overlaps the row translated from a `related` relationship and fails to load (ADR-032).
- Given a `relation-existence` rule, when translated, then the edge carries `from = type`, `to = "*"`, `via = "*"`, `required = severity`.
- Given a relationship carrying `traversal`, when translated, then a wildcard row is emitted with `from = "*"`, `to = "*"`, `via = name`, and that traversal role.
- Given any repository, when `validate` runs before and after migration, then the finding set is identical.
- Given a config already carrying `[[edges]]` and no `[[rules]]`, when `fix --config` runs, then the file is untouched.
- Given a `[[rules]]` block with an attached comment, when the migration plan is shown, then it states that comments on translated blocks do not survive — before anything is applied.
- Given sections the migration does not understand, when it runs, then they are preserved.

## Notes

The wildcard rows this emits — the `relation-existence` translations and the relationship traversal markers — are exactly the imprecision the edge table lets authors escape. Migration lands a working config, not a good one; narrowing is a later human edit.

`ops/fix/config.rs`'s append-only doc comment is no longer accurate for the module and must be amended in the same change — a stale rule is either changed or the code is (convention §Governance).
