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

- each `parent-child` rule becomes an edge per chain-marked relationship, with `from = child`, `to = [parent]`, `via = ` that relationship's name, `traversal = "chain"`, and `required = severity`
- each `relation-existence` rule becomes `from = type`, `to = "*"`, `via = "*"`, `required = severity`
- each relationship carrying `traversal` contributes a wildcard row: `from = "*"`, `to = "*"`, `via = name`, and that traversal role
- the source `[[rules]]` blocks and `traversal` keys are deleted

A translated parent-child row names the chain relationship in `via` rather than carrying `via = "*"`. This decision was amended after the original was found to be wrong on its own terms and unloadable besides.

The original reasoning was that `via = "*"` preserves today's behaviour, "which accepts any chain relationship". It does not: `via = "*"` accepts *any* relationship, while `validation.rs:583` satisfies a parent-child rule only through a relationship the config marks `traversal = "chain"`. A story linked to an RFC by `blocks` fails the rule today and would pass under `via = "*"`. The wildcard is the widening, and naming the chain relationship is the preservation.

It is also the only shape that loads. A `via = "*"` row carrying `traversal = "chain"` overlaps the wildcard row translated from a `related` relationship on all three positions, and `reject_traversal_disagreements` (`config.rs:162`) refuses the pair — for the default config, this repo's own included. Naming the relationship in `via` removes the overlap: the row agrees with its own relationship's marker row and cannot match any other.

`via` is one name or `"*"`, never a set, so a config marking two relationships as chain yields one translated row per relationship rather than one row naming both.

Sections the migration does not understand are preserved. The migration is idempotent: a config already carrying `[[edges]]` and no `[[rules]]` is left untouched.

## Consequences

- Migration is behaviour-preserving, so upgrading cannot break a repository's validation state. Naming the chain relationship in `via` is what makes that true; the wildcard would have widened it.
- Byte-for-byte preservation is lost for the blocks being replaced. Comments attached to a `[[rules]]` block do not survive translation; the migration plan must say so before applying.
- `ops/fix/config.rs`'s append-only contract is no longer accurate for the whole module and its doc comment needs amending, per the convention's governance rule that a stale rule is either changed or the code is.
- The wildcard rows emitted for relationship traversal are exactly the imprecision the edge table was meant to allow escaping. Migration lands a working config, not a good one; narrowing is left to the author.
