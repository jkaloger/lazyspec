---
title: "Subdirectory children have inconsistent identity across resolvers"
type: bug
status: reported
author: "Jack Kaloger"
date: 2026-09-28
tags: []
related: []
---

## Expected

A child created with `create --parent CHG-001` gets an id like `DELTA-001`, and every command that takes an id resolves it the same way. Whatever id `list --json` reports for a document, `show <id>` accepts.

## Actual

Prototype config: a `change` type with `subdirectory = true`, a `delta` type, then `create delta "Vehicle search delta" --parent CHG-001`.

```
$ lazyspec list --json | jq '.[0].id'
"DELTA-001-vehicle-search-delta"
$ lazyspec show DELTA-001
Error: document not found: DELTA-001
$ lazyspec show DELTA-001-vehicle-search-delta
Error: document not found: DELTA-001-vehicle-search-delta
$ lazyspec show CHG-001/DELTA-001          # works
$ lazyspec link SPEC-001 modifies CHG-001/DELTA-001
{"target": "DELTA-001-vehicle-search-delta"}   # written to frontmatter; validate passes; show refuses it
```

Same-type children are worse: `create change "Nested" --parent CHG-001` yields id `CHG-001-nested-change`, indistinguishable by prefix from the parent.

## Cause

Two resolver sites disagree about what a child's id is.

1. `extract_id` in `src/engine/store.rs`: for a file whose parent folder carries a prefixed id (`CHG-001-lifestyle-filter/`), it returns the full filename stem instead of running `extract_id_from_name` on it. So the child id is `DELTA-001-vehicle-search-delta`.
2. `resolve_unqualified` in `src/engine/store.rs` filters `!self.parent_of.contains_key(&d.path)`, so no bare id reaches a child at all. Only the `PARENT/child` form in `resolve_shorthand` does. `resolve_relation_target` matches `d.id == target` across every document including children, which is why `link` writes a target that `show` cannot resolve.

## Fix

- Derive a child's id as `extract_id_from_name(stem)` so it is `DELTA-001` (numbering is already per parent directory).
- Let `resolve_unqualified` include children. A bare child id that is unique across the store resolves; one that collides across parents returns `Ambiguous` listing the `PARENT/child` forms.
- `link` writes the same id `show` accepts. Add an integration test that round-trips `list --json` ids through `show` for a nested fixture.
