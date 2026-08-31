---
title: Consolidated edge table for the document DAG
type: rfc
status: accepted
author: Jack Kaloger
date: 2026-08-25
tags: []
related:
- related-to: RFC-042
---
## Problem

Hierarchy and linkage are spread across seven config surfaces at three levels, and the word "parent" names three unrelated things.

| Surface | Level | Says | Read by |
|---|---|---|---|
| `[[relationships]]` `name`/`inverse` | instance | edge vocabulary | `link`, frontmatter validation |
| `[[relationships]].traversal` | instance | which walk the edge joins | `store.rs:114-123`, `context.rs` |
| `[[relationships]].github_native` | instance | native store edge | `sync.rs` |
| rule `parent-child` `{child,parent}` | type | child needs *some* chain edge to parent | `validation.rs:524-559`, `prompt.rs:177` |
| `parent-child.require_parent_status` | type | parent must reach status before `create` | `ops/create.rs:87-99` |
| rule `relation-existence` | type | type needs >=1 relation | `validation.rs:561+` |
| `types[].parent_type` | type | containment: shared store backend, dir nesting | `store.parent_of`, `cli/convention.rs:22` |

Three defects follow.

**1. No config row owns an edge.** A `parent-child` rule names a type pair but never the relationship that realizes it. `validation.rs:534-548` checks `store.chain_relationships.contains(rel_type) && target.doc_type == parent` — so *any* relationship marked `traversal = "chain"` satisfies *any* parent-child rule. This project marks both `implements` and `targets` as chain, so `iteration --targets--> STORY-250` satisfies `iterations-need-stories`. Nobody wrote that rule.

This is a regression against RFC-042, not a new gap. RFC-042 §Design.2 specifies that type-pair constraints stay in `[[rules]]` "referencing relationships by name", and its interface sketch carries `link = "implements"` on the rule. `ValidationRule::ParentChild` (`config.rs:31-38`) has no such field. STORY-127 and STORY-128 shipped without it.

**2. `parent` is scalar, so disjunction has nowhere to live.** An iteration should be allowed to implement a spike, *or* a story, *or* a bug. `parent: String` (`config.rs:34`) admits exactly one type name, and no other surface expresses a type set.

**3. `traversal` sits on the wrong object.** `RelationshipDef.traversal` (`config.rs:418`) is a global property of a relationship *name*. Whether an edge is hierarchy is a property of the triple (relationship, from-type, to-type): `targets` is genuine hierarchy for iteration→milestone and accidental hierarchy for every other pair.

Compounding all three: `rule.parent` (a type constraint on chain edges), `TypeDef.parent_type` (store backend and directory containment, `config.rs:320`), and `Store.parent_of` (resolved path nesting, `store.rs:342`) share a name and mean different things. Only the latter two concern where a document lives.

## Intent

One config table owns the document DAG. Each row declares a directed edge kind: source type, permitted target types, the relationship that realizes it, whether it participates in context traversal, and whether its absence is a validation finding.

`[[relationships]]` shrinks to what it is good at — vocabulary and native-store mapping. `parent_type` stays, scoped explicitly to containment. Set-valued targets fall out of the table's shape rather than being bolted onto a scalar field.

## Design

Resolved decisions, each with an ADR:

1. **Edges are a first-class table.** `[[edges]]` replaces both rule shapes and absorbs `traversal` from `RelationshipDef`. `parent_type` is untouched and documented as containment-only. (ADR-030)
2. **Wildcard endpoints, specific-over-wildcard.** `from`/`to`/`via` accept `"*"`. A wildcard row and a specific row for the same triple compose for traversal; for requiredness the most specific row wins. (ADR-031)
3. **Migration is a mechanical translation.** `fix --config` rewrites `[[rules]]` plus `relationships.traversal` into `[[edges]]`, following ADR-012's lenient-read precedent. (ADR-032)

Decided by precedent:

- **`via = "*"` is explicit.** Absent `via` meaning "any relationship" would smuggle a second rule into the table's shape. `relation-existence` translates to `via = "*"`, `to = "*"`, `required = "error"`.
- **`required` semantics for a set are "any one".** `to = ["spike","story","bug"]` with `required = "error"` is satisfied by one edge to any one member, not one edge to each.
- **`required` is `Option<Severity>`.** Absent means the edge is legal and may walk, but its absence is not a finding — the current `traversal`-only relationships (`related-to`, `targets`) translate to exactly that.
- **The edge table does not constrain `link`.** RFC-042 §Design.2 holds: relations stay arbitrary doc→doc at the model level (`related: Vec<Relation>` on every `DocMeta`), and constraints remain a validation-layer concern. `link` continues to reject only unknown *relationship names* via the registry. An edge absent from the table is a finding, never a refused command.
- **No edge condition refuses a command.** Every unsatisfied edge is a validation finding. Status-conditioned `create` gating is abandoned rather than carried onto the edge table: ADR-033 supersedes ADR-022, and the scalar `require_parent_status` dies with `[[rules]]` in STORY-259 with no successor. The edge table therefore has one policy, not two.

### The traversal cost, stated plainly

Today `traversal = "chain"` on `implements` is blanket: every type pair inherits it free. Per-edge traversal has no blanket, so each walking edge needs a row, and without wildcards one line becomes N-squared lines. Wildcards make it tractable but reintroduce the imprecision the change was meant to remove: `from = "*"` on `targets` means any type may target a milestone, which is the blanket behaviour again.

The honest claim is therefore narrower than "fixes traversal": the table is **precise where a row is spent and blanket where it is not**. That is a real improvement over a global flag, because precision becomes available per pair instead of impossible.

## Interface sketch

```toml
@draft [[relationships]]              # vocabulary + native mapping only
name = "implements"
inverse = "implemented-by"

[[edges]]
name = "iterations-implement-work"
from = "iteration"
to   = ["spike", "story", "bug"]
via  = "implements"
traversal = "chain"
required  = "error"

[[edges]]
name = "general-relatedness"
from = "*"
to   = "*"
via  = "related-to"
traversal = "related"
```

```rust
@draft pub struct EdgeDef {
    pub name: String,
    pub from: TypeSelector,
    pub to: TypeSelector,
    pub via: RelSelector,
    pub traversal: Option<Traversal>,
    pub required: Option<Severity>,
}

@draft pub enum TypeSelector { Any, Types(Vec<String>) }
@draft pub enum RelSelector { Any, Named(String) }
```

`@ref src/engine/config.rs#ValidationRule` — both variants replaced by `EdgeDef`.
`@ref src/engine/config.rs#RelationshipDef` — loses `traversal`.
`@ref src/engine/store.rs#Store` — `chain_relationships: Vec<String>` cannot stay a flat name list; the walk needs from/to types at each hop.

## Stories

Decomposed vertically: each slice is observable by a real actor -- a DAG designer configuring types, a document author running `create`/`validate`/`context`, an agent consuming `--json`, or a maintainer upgrading an existing project. An earlier draft of this section sliced by layer (schema, validation, traversal, TUI, CLI); that backlog delivered nothing observable until two slices had both landed, and was replaced.

1. **STORY-254 -- Declare an edge whose target may be any of several types.** The walking skeleton: `[[edges]]` parses and `validate` enforces it, with `from`/`to`/`via`/`required` only. `[[rules]]` keeps working alongside so the table can land incrementally.
2. **STORY-256 -- Wildcard edge endpoints for a short starter config.** `"*"` on any position, with specificity and contradiction rules.
3. **STORY-257 -- Walk the document DAG from the edge table.** The heavy slice: traversal off `RelationshipDef`, across CLI, TUI graph, and web view.
4. **STORY-258 -- Migrate an existing config to the edge table.** Behaviour-preserving translation in `fix --config`.
5. **STORY-259 -- Retire the rules table.** Closes the dual-declaration window STORY-254 opens, and takes `require_parent_status` with it.
6. **STORY-260 -- Edit edges in the TUI settings panel.**
7. **STORY-261 -- Edit edges from the config CLI and `init`.**
8. **STORY-262 -- Derive agent type boundaries from the edge table.** Collapses the `/lazy` and `/execute` union-of-two-sources instruction.

STORY-255 (gate creation on the target's status) is withdrawn -- see ADR-033.

Blocking: 254 gates 256, 257, 258, 260, 261. 256 also gates 258, which gates 259. 257 gates 262.

README, `--help`, and JSON-schema updates are acceptance criteria on whichever slice changes that surface, not a story of their own. Layering (dictum 3) and `--json` coverage (dictum 2) are project-wide constraints, not slices.

## Open questions

- ~~Does `required` on a `from = "*"` row mean anything?~~ Closed by ADR-031: rejected at load, since it would demand the edge from every declared type. Story 1 implements the check.
- `spike` does not exist as a type in this project (types are convention, dictum, spec, rfc, story, iteration, adr, audit, bug, milestone, clickup). Adding it is a separate config change, out of scope here.
- `prompt.rs:177 child_types_for` derives child types by scanning rules for `parent == doc_type`. Under edges this becomes a reverse index; whether it is built eagerly in `Store` or computed per call is a story-3 detail.
