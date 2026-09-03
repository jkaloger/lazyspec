---
title: Wildcard endpoints on the edge table
type: iteration
status: complete
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-256
- blocks: ITERATION-369
- blocks: ITERATION-370
---

## Objective

`from`, `to`, and `via` on an `[[edges]]` row accept `"*"`, parsed into the selector types RFC-067 sketches; a row that omits `via` fails load with a message that names `via = "*"` as the way to say "any relationship".

## Satisfies

STORY-256 AC5. AC1, AC2, AC3, AC4, AC6 deferred — see Out of scope.

## Context

- Story + ACs: STORY-256
- What a wildcard means per position, and why an absent `via` is refused rather than read as "any": ADR-031 §Decision; RFC-067 §Design (bullet "`via = \"*\"` is explicit")
- Selector shape: RFC-067 §Interface sketch (`TypeSelector`, `RelSelector`)
- The `EdgeDef` this widens landed in ITERATION-366; the checker that reads it landed in ITERATION-367
- Touch:
  - `src/engine/config.rs` — `EdgeDef` (`from`/`to`/`via`), `deserialize_edge_targets` + the `EdgeTargets` untagged enum, the strict-load unknown-type/unknown-relationship loop in `Config::parse`, the JSON-schema test
  - `README.md` §`[[edges]]` — the closing paragraph currently asserts wildcards are "not supported yet"
- `Config::to_toml` serialises `Config` itself, so selectors must round-trip to the same TOML spelling a human wrote (`"*"`, `"story"`, `["story", "bug"]`). `config --json` and the config writers in `src/cli/config.rs` both depend on that.

## Tasks

1. Test-first: `from = "*"`, `to = "*"`, `via = "*"` parse to the `Any` variants, and the existing scalar/list forms of `to` keep parsing as they do today (ITERATION-366 Task 1's assertion must stay green).
2. Replace `EdgeDef`'s `from: String`, `to: Vec<String>`, `via: String` with `TypeSelector`/`RelSelector` per RFC-067 §Interface sketch. This is the second concrete use dictum 6 was waiting for — ITERATION-366 Task 2 deliberately deferred the enums to here.
3. Serde both directions: `"*"` ↔ `Any`, a bare name ↔ a one-element `Types`, a list ↔ `Types`. Extend the schema assertion so all three spellings are documented, and assert a `to_toml` round-trip — a `"*"` that comes back as `["*"]` is a bug the config editors would propagate.
4. Test-first: an edge row with no `via` key fails load, naming the offending edge and stating that `via = "*"` is how to mean "any relationship". Serde's own "missing field `via`" says neither, so the raw shape needs the field optional and an explicit `bail!` in `Config::parse`.
5. Make the existing unknown-type and unknown-relationship checks skip wildcard positions — `"*"` names no declared type and must not be reported as one.
6. README `[[edges]]` reference: wildcards are accepted on all three positions, `via` is mandatory and `"*"` is its explicit any-form. Drop the "not supported yet" sentence for wildcards and the "exactly the five keys" phrasing where it now misleads; leave the `traversal` caveat alone (STORY-257).

## Out of scope

- A wildcard row actually matching anything during `validate` (AC1, AC6) → next iteration.
- Rejecting `required` on a wildcard `from`, and equal-specificity contradictions (AC3, AC4) → the load-rejection iteration.
- Most-specific-row resolution (AC2) → the last iteration on this story.
- `traversal` on the edge → STORY-257. `fix --config` → STORY-258. Retiring `[[rules]]` → STORY-259. `init` emitting the short starter config this story's title promises → STORY-261; this slice only makes that config expressible.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: the widened shape must survive `config --json` and the emitted schema. Dictum 3: the selectors are engine types.

## Verification

`cargo run -- config --json | jq .edges` still returns `[]` on this repo, and a scratch `.lazyspec.toml` carrying `from = "*"`, `to = "*"`, `via = "related-to"` loads and re-emits those three values unchanged through `to_toml`.
