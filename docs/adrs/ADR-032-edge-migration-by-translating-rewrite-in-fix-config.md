---
title: Edge migration by translating rewrite in fix --config
type: adr
status: accepted
author: Jack Kaloger
date: 2026-08-25
tags: []
related:
- related-to: RFC-067
- related-to: ADR-012
---
## Context

RFC-067 changes the shape of every config that declares rules or relationship traversal. ADR-012 set the precedent for this situation: `fix --config` with a lenient read that bypasses strict load, so a stale config can be repaired by the tool that rejects it, and the strict-load error names the remedy.

That precedent does not transfer cleanly. `collect_config_fixes` (`ops/fix/config.rs:34-41`) is documented as append-only by design: the existing file is preserved byte-for-byte and only missing blocks are appended, which keeps `[github]`, comments, and ordering intact. The edge migration cannot be append-only — `[[rules]]` blocks and `relationships.traversal` keys must be *removed*, or the config carries two contradictory DAG declarations.

## Decision

`fix --config` gains a translating migration that rewrites rather than appends. The translation is mechanical and total:

- each `parent-child` rule becomes one edge, with `from = child`, `to = [parent]`, `via = ` the set of chain-marked relationship names, `traversal = "chain"`, and `required = severity`
- each `relation-existence` rule becomes `from = type`, `to = "*"`, `via = "*"`, `required = severity`
- each relationship carrying `traversal` contributes a wildcard row: `from = "*"`, `to = "*"`, `via = name`, and that traversal role
- the source `[[rules]]` blocks and `traversal` keys are deleted

A translated parent-child row names the chain relationship in `via` rather than carrying `via = "*"`. This decision was amended after the original was found to be wrong on its own terms and unloadable besides.

The original reasoning was that `via = "*"` preserves today's behaviour, "which accepts any chain relationship". It does not: `via = "*"` accepts *any* relationship, while `validation.rs:583` satisfies a parent-child rule only through a relationship the config marks `traversal = "chain"`. A story linked to an RFC by `blocks` fails the rule today and would pass under `via = "*"`. The wildcard is the widening, and naming the chain relationship is the preservation.

It is also the only shape that loads. A `via = "*"` row carrying `traversal = "chain"` overlaps the wildcard row translated from a `related` relationship on all three positions, and `reject_traversal_disagreements` (`config.rs:162`) refuses the pair — for the default config, this repo's own included. Naming the relationship in `via` removes the overlap: the row agrees with its own relationship's marker row and cannot match any other.

`via` takes a set of relationship names, exactly as `to` takes a set of type names, and a config marking two relationships as chain yields one translated row naming both. This is the second amendment to this decision, made when a row per relationship was measured against the checker it replaces.

A config marking *no* relationship chain yields `via = []`. Such a rule is satisfied by nothing today, so `validation.rs` fires it on every child document; the empty set is the `via` that goes on doing that, where dropping the rule would silence the whole set of findings the migration promises to preserve. The row loads: an empty `via` names no relationship for the declared-relationship check to look up, and it intersects nothing, so it can neither tie on requiredness nor disagree on traversal.

One row per chain relationship changes the quantifier. `validation.rs:583` satisfies a parent-child rule if *any* chain relationship reaches a parent of the right type — a disjunction. Two rows are two independent demands: equal specificity and disjoint `via` means neither displaces the other in `undisplaced_demands` (`validation.rs:641`), so both stand and the document needs *both* links. On this repository's own config, which marks `implements` and `targets` chain, that gives every story a warning naming whichever of the two it did not use.

A set in `via` is a disjunction over its members, which is what `to` already means and what the old checker meant. It also keeps the overlap argument above intact: the row names relationships rather than matching by wildcard, so it still cannot collide with a `related` relationship's marker row.

Sections the migration does not understand are preserved. The migration is idempotent: a config already carrying `[[edges]]` and no `[[rules]]` is left untouched.

## Consequences

- Migration is behaviour-preserving, so upgrading cannot break a repository's validation state. Naming the chain relationships in `via` is what makes that true: the wildcard would have widened the rule, and a row apiece would have narrowed it to their conjunction.
- `via` becoming set-valued is a schema change, not only a migration one. `RelSelector` gains the shape `TypeSelector` already has, and every surface that reads or writes a `via` — the loader's declared-relationship check, the config CLI, the TUI settings panel, the JSON schema, the README — follows it.
- Byte-for-byte preservation is lost for the blocks being replaced. Comments attached to a `[[rules]]` block do not survive translation; the migration plan must say so before applying.
- `ops/fix/config.rs`'s append-only contract is no longer accurate for the whole module and its doc comment needs amending, per the convention's governance rule that a stale rule is either changed or the code is.
- The wildcard rows emitted for relationship traversal are exactly the imprecision the edge table was meant to allow escaping. Migration lands a working config, not a good one; narrowing is left to the author.
