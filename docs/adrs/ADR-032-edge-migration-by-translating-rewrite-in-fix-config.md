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

- each `parent-child` rule becomes an edge with `from = child`, `to = [parent]`, `traversal = "chain"`, `required = severity`, and `via = "*"`
- each `relation-existence` rule becomes `from = type`, `to = "*"`, `via = "*"`, `required = severity`
- each relationship carrying `traversal` contributes a wildcard row: `from = "*"`, `to = "*"`, `via = name`, and that traversal role
- the source `[[rules]]` blocks and `traversal` keys are deleted

`via = "*"` on translated parent-child rules is deliberate. It preserves today's actual behaviour, which accepts any chain relationship, instead of silently tightening to `implements` and turning existing valid documents into findings. Tightening is a subsequent human edit, not something migration does.

Sections the migration does not understand are preserved. The migration is idempotent: a config already carrying `[[edges]]` and no `[[rules]]` is left untouched.

## Consequences

- Migration is behaviour-preserving, so upgrading cannot break a repository's validation state. The `targets`-satisfies-`implements` hole survives migration by design and closes only when a human writes a concrete `via`.
- Byte-for-byte preservation is lost for the blocks being replaced. Comments attached to a `[[rules]]` block do not survive translation; the migration plan must say so before applying.
- `ops/fix/config.rs`'s append-only contract is no longer accurate for the whole module and its doc comment needs amending, per the convention's governance rule that a stale rule is either changed or the code is.
- The wildcard rows emitted for relationship traversal are exactly the imprecision the edge table was meant to allow escaping. Migration lands a working config, not a good one; narrowing is left to the author.
